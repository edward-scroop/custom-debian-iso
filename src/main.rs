// SPDX-FileCopyrightText: Copyright 2026 Edward Scroop <edward.scroop@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

mod config;
mod fs;
mod iso;

use crate::{
    config::{
        get_config_script_from_toml, get_custom_minijinja_environment,
        get_install_script_from_toml, get_preseed_from_toml, get_template_minijinja_environment,
        load_toml, process_files_preseed, process_files_toml, toml_get_string,
        write_string_to_file,
    },
    fs::{check_if_dir_exists, check_if_file_exists, delete_dir, prepare_temp_and_output_dir},
    iso::{create_iso, enable_bios_booting, enable_uefi_booting, extract_iso},
};
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};
use toml::Value;

fn main() -> ExitCode {
    /// A program to create custom debian isos.
    #[derive(Parser, Debug)]
    #[command(version, about, long_about = None)]
    struct Args {
        /// Path of the custom iso config directory. Is relative to the `work_dir`. Will search for a
        /// config.toml located in the directory.
        custom_dir: String,

        /// Directory to config and isos. Will use current working dir if not set.
        work_dir: Option<String>,
    }

    let args = Args::parse();

    let work_dir = if let Some(dir) = args.work_dir {
        // TODO: Handle missing dir or invalid dir perms.
        PathBuf::from(dir)
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(e) => {
                eprintln!("Error getting current working directory: {e}");

                return ExitCode::FAILURE;
            }
        }
    };

    let output_dir = work_dir.join("output");
    let temp_dir = work_dir.join(".tmp");
    let isos_dir = work_dir.join("isos");
    let scripts_dir = work_dir.join("scripts");
    let templates_dir = work_dir.join("templates");
    let files_dir = work_dir.join("files");
    let custom_dir = work_dir.join(args.custom_dir);
    let custom_iso_name = custom_dir
        .file_name()
        .expect("custom dir should a directory")
        .to_string_lossy()
        .to_string();
    let custom_scripts_dir = custom_dir.join("scripts");
    let custom_files_dir = custom_dir.join("files");
    let mut output_path = output_dir.join(&custom_iso_name);
    output_path.add_extension("iso");

    for dir in [
        &work_dir,
        &isos_dir,
        &scripts_dir,
        &templates_dir,
        &files_dir,
        &custom_dir,
        &custom_scripts_dir,
        &custom_files_dir,
    ] {
        if let Err(e) = check_if_dir_exists(dir) {
            eprintln!("{e}");

            return ExitCode::FAILURE;
        }
    }

    if let Err(e) = prepare_temp_and_output_dir(&temp_dir, &output_dir) {
        eprintln!("{e}");

        return ExitCode::FAILURE;
    }

    let config_file = custom_dir.join("config.toml");
    let mut config_toml = match load_toml(&config_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting config: {e}");

            return ExitCode::FAILURE;
        }
    };

    let template_path = match toml_get_string("template", &mut config_toml) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting template: {e}");

            return ExitCode::FAILURE;
        }
    };
    let template_file = templates_dir.join(&template_path);
    let mut template_toml = match load_toml(&template_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting template: {e}");

            return ExitCode::FAILURE;
        }
    };

    let mut preseed_toml = match get_preseed_from_toml(
        template_toml.get("preseed"),
        config_toml.get("preseed"),
        &template_file,
        &config_file,
    ) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}",);

            return ExitCode::FAILURE;
        }
    };

    let template_environment = get_template_minijinja_environment(&work_dir);
    let custom_environment = get_custom_minijinja_environment(&work_dir, custom_iso_name);

    let config_script = match get_config_script_from_toml(
        template_toml.get("config"),
        config_toml.get("config"),
        &scripts_dir,
        &template_environment,
        &custom_environment,
    ) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting config script values: {e}",);

            return ExitCode::FAILURE;
        }
    };

    let install_script = match get_install_script_from_toml(
        config_toml.get("install"),
        &scripts_dir,
        &custom_environment,
    ) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting install script values: {e}",);

            return ExitCode::FAILURE;
        }
    };

    let files_toml = match process_files_toml(template_toml.get_mut("files"), &files_dir) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error getting files from template: {}: {e}",
                template_file.to_string_lossy().to_string()
            );

            return ExitCode::FAILURE;
        }
    };
    let custom_files_toml = match process_files_toml(config_toml.get_mut("files"), &files_dir) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error getting files from config: {}: {e}",
                config_file.to_string_lossy().to_string()
            );

            return ExitCode::FAILURE;
        }
    };

    let iso_path = match toml_get_string("iso", &mut template_toml) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting iso path: {e}");

            return ExitCode::FAILURE;
        }
    };
    let iso_file = isos_dir.join(iso_path);

    if let Err(e) = check_if_file_exists(&iso_file) {
        eprintln!("error checking for iso file: {e}");

        return ExitCode::FAILURE;
    }

    if let Err(e) = extract_iso(&iso_file, &temp_dir) {
        eprintln!("error extracting iso: {e}");

        return ExitCode::FAILURE;
    }

    let early_command_key = "d-i preseed/early_command string";
    let late_command_key = "d-i preseed/late_command string";
    let mut early_command_value = String::new();
    let mut late_command_value = String::from(concat!(
        "cp /cdrom/config.sh /target/bin; ",
        "cp /cdrom/install.sh /target/bin; ",
        "cp /cdrom/install.service /target/etc/systemd/system; ",
        "cp /cdrom/install.timer /target/etc/systemd/system; ",
        "chmod +x /bin/config.sh /bin/install.sh; ",
        "in-target /bin/config.sh; ",
        "in-target systemctl enable install.timer; ",
    ));

    if let Err(e) = process_files_preseed(
        files_toml.iter().chain(custom_files_toml.iter()),
        &mut early_command_value,
        &mut late_command_value,
        &temp_dir,
    ) {
        eprintln!("{e}");

        return ExitCode::FAILURE;
    }

    if let Some(value) = preseed_toml.remove(early_command_key) {
        early_command_value.push(' ');
        early_command_value.push_str(
            value
                .as_str()
                .expect("table should have been checked to be only strings"),
        );
    }
    if let Some(value) = preseed_toml.remove(late_command_key) {
        late_command_value.push(' ');
        late_command_value.push_str(
            value
                .as_str()
                .expect("table should have been checked to be only strings"),
        );
    }

    preseed_toml.insert(early_command_key.into(), Value::String(early_command_value));
    preseed_toml.insert(late_command_key.into(), Value::String(late_command_value));

    let mut preseed_string = String::from("#_preseed_V1\n");
    for (key, value) in preseed_toml {
        preseed_string.push_str(&format!(
            "{key} {}\n",
            value
                .as_str()
                .expect("table should have been checked to be only strings")
        ));
    }

    let preseed_file_path = temp_dir.join("preseed.cfg");
    if let Err(e) = write_string_to_file(&preseed_file_path, &preseed_string, false) {
        eprintln!(
            "error writing preseed file to {}: {e}",
            preseed_file_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    let config_script_path = temp_dir.join("config.sh");
    if let Err(e) = write_string_to_file(&config_script_path, &config_script, true) {
        eprintln!(
            "error writing config script to {}: {e}",
            config_script_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    let install_script_path = temp_dir.join("install.sh");
    if let Err(e) = write_string_to_file(&install_script_path, &install_script, true) {
        eprintln!(
            "error writing install script to {}: {e}",
            install_script_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    let install_service_literal = concat!(
        "[Unit]\n",
        "Description=Install script\n",
        "Requires=network-online.target\n",
        "After=network-online.target\n",
        "\n",
        "[Service]\n",
        "Type=oneshot\n",
        "ExecStart=/bin/install.sh\n",
        "StandardOutput=append:/var/log/install.log\n",
        "StandardError=append:/var/log/install.log\n",
        "User=root\n",
        "Group=root\n",
        "Environment=TERM=linux\n",
        "\n",
        "[Install]\n",
        "WantedBy=multi-user.target\n",
    );
    let install_service_path = temp_dir.join("install.service");
    if let Err(e) = write_string_to_file(&install_service_path, &install_service_literal, false) {
        eprintln!(
            "error writing install service file to {}: {e}",
            install_service_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    let install_timer_literal = concat!(
        "[Unit]\n",
        "Description=Runs install script after boot\n",
        "\n",
        "[Timer]\n",
        "OnBootSec=1m\n",
        "Unit=install.service\n",
        "\n",
        "[Install]\n",
        "WantedBy=multi-user.target\n",
    );
    let install_timer_path = temp_dir.join("install.timer");
    if let Err(e) = write_string_to_file(&install_timer_path, &install_timer_literal, false) {
        eprintln!(
            "error writing install timer file to {}: {e}",
            install_timer_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    if let Err(e) = enable_bios_booting(&temp_dir) {
        eprintln!("error enabling bios booting: {e}");

        return ExitCode::FAILURE;
    }

    if let Err(e) = enable_uefi_booting(&temp_dir) {
        eprintln!("error enabling uefi booting: {e}");

        return ExitCode::FAILURE;
    }

    if let Err(e) = create_iso(&iso_file, &temp_dir) {
        eprintln!("error creating iso: {e}");

        return ExitCode::FAILURE;
    }

    if let Err(e) = delete_dir(&temp_dir) {
        eprintln!(
            "error deleting temp directory: {} {}",
            e,
            temp_dir.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
