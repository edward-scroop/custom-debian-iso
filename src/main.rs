// SPDX-FileCopyrightText: Copyright 2026 Edward Scroop <edward.scroop@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use clap::Parser;
use minijinja::context;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use toml::{Table, Value, value::Array};

enum CheckType {
    File,
    Dir,
}

fn check_if_exists(file: &Path, check_type: &CheckType) -> Result<(), String> {
    match file.try_exists() {
        Ok(exists) => {
            if !exists {
                return Err(format!(
                    "{}: {} does not exist",
                    {
                        match check_type {
                            CheckType::File => "file",
                            CheckType::Dir => "directory",
                        }
                    },
                    file.to_string_lossy()
                ));
            }
        }
        Err(e) => {
            return Err(format!(
                "Cannot check if {}: {} exists: {e}",
                {
                    match check_type {
                        CheckType::File => "file",
                        CheckType::Dir => "directory",
                    }
                },
                file.to_string_lossy()
            ));
        }
    }
    let metadata = match file.symlink_metadata() {
        Ok(x) => x,
        Err(e) => {
            return Err(format!(
                "Error getting {} metadata: {e}",
                file.to_string_lossy()
            ));
        }
    };

    let is_correct_type = match check_type {
        CheckType::File => metadata.is_file(),
        CheckType::Dir => metadata.is_dir(),
    };

    if !is_correct_type {
        if metadata.is_symlink() {
            return Err(format!("Error {} is a symlink", file.to_string_lossy()));
        }

        return Err(format!("Error {} is a {}", file.to_string_lossy(), {
            match check_type {
                CheckType::File => "file",
                CheckType::Dir => "directory",
            }
        }));
    }

    Ok(())
}

fn check_if_file_exists(file: &Path) -> Result<(), String> {
    check_if_exists(file, &CheckType::File)
}

fn check_if_dir_exists(dir: &Path) -> Result<(), String> {
    check_if_exists(dir, &CheckType::Dir)
}

fn toml_get_string(key_name: &str, toml_table: &mut Table) -> Result<String, String> {
    if let Some(toml) = toml_table.remove(key_name) {
        if toml.is_str() {
            match toml {
                Value::String(string) => Ok(string),
                _ => unreachable!("toml is checked to be string already"),
            }
        } else {
            Err(format!(
                "{key_name} is not a toml string, is: {} instead",
                toml.type_str()
            ))
        }
    } else {
        Err(format!("key: {key_name} missing from table"))
    }
}

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

    if let Err(e) = fs::remove_dir_all(&temp_dir)
        && e.kind() != io::ErrorKind::NotFound
    {
        eprintln!(
            "error deleting temp directory: {e} {}",
            temp_dir.to_string_lossy()
        );
        return ExitCode::FAILURE;
    }

    if let Err(e) = fs::create_dir(&temp_dir) {
        eprintln!(
            "error creating temp directory: {e} {}",
            temp_dir.to_string_lossy()
        );
        return ExitCode::FAILURE;
    }

    if let Err(e) = fs::create_dir(&output_dir)
        && e.kind() != io::ErrorKind::AlreadyExists
    {
        eprintln!(
            "error creating output directory: {e} {}",
            output_dir.to_string_lossy()
        );
        return ExitCode::FAILURE;
    }

    let config_file = custom_dir.join("config.toml");
    let config_string = match fs::read_to_string(&config_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error getting config: {e} {}",
                config_file.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };
    let mut config_toml: Table = match toml::from_str(&config_string) {
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
    let template_string = match fs::read_to_string(&template_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error getting template: {e} {}",
                template_file.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };
    let mut template_toml: Table = match toml::from_str(&template_string) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting template: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut preseed_toml: Table = match template_toml.get("preseed") {
        Some(x) => {
            if let Some(table) = x.as_table() {
                for (key, value) in table {
                    if !value.is_str() {
                        eprintln!(
                            "error template preseed value {key} in {}: is not a string",
                            template_file.to_string_lossy()
                        );
                        return ExitCode::FAILURE;
                    }
                }

                table.clone()
            } else {
                eprintln!(
                    "error getting preseed values from template {}: is not a table",
                    template_file.to_string_lossy()
                );
                return ExitCode::FAILURE;
            }
        }
        None => Table::new(),
    };
    if let Some(x) = config_toml.get("preseed") {
        if let Some(table) = x.as_table().cloned() {
            for (key, value) in &table {
                if !value.is_str() {
                    eprintln!(
                        "error config preseed value {key} in {}: is not a string",
                        config_file.to_string_lossy()
                    );
                    return ExitCode::FAILURE;
                }
            }

            preseed_toml.extend(table);
        } else {
            eprintln!(
                "error getting preseed values from config {}: is not a table",
                config_file.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    }

    let mut template_environment = minijinja::Environment::new();
    template_environment.set_auto_reload(false);
    template_environment.set_loader(minijinja::path_loader(&scripts_dir));

    let mut custom_environment = minijinja::Environment::new();
    custom_environment.set_auto_reload(false);
    custom_environment.set_loader(minijinja::path_loader(&work_dir));
    custom_environment.set_path_join_callback(move |name, _parent| {
        let mut path = name.split('/').collect::<Vec<&str>>();
        let mut added_path: Vec<&str> = Vec::new();

        if *path
            .first()
            .expect("vec should contain atleast one element")
            == "shared"
        {
            path.remove(0);
        } else {
            added_path.push(&custom_iso_name);
        }
        added_path.push("scripts");

        for new_path in added_path.iter().rev() {
            path.insert(0, *new_path);
        }
        path.join("/").into()
    });

    let mut template_missing_config_script = false;
    let mut config_script = if let Some(config) = template_toml.get("config") {
        let Some(table) = config.as_table() else {
            eprintln!(
                "error getting config script values from template {}: is not a table",
                template_file.to_string_lossy()
            );
            return ExitCode::FAILURE;
        };

        if let Some(script) = table.get("script-file") {
            let Some(script_path) = script.as_str() else {
                eprintln!(
                    "error getting config script file from template {}: script-file is not a string",
                    template_file.to_string_lossy()
                );
                return ExitCode::FAILURE;
            };

            let script_path = scripts_dir.join(script_path.trim_start_matches('/'));
            match fs::read_to_string(&script_path) {
                Ok(script_value) => {
                    match template_environment.render_str(&script_value, context!()) {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!(
                                "error parsing config script jinja from template: {e} {}",
                                script_path.to_string_lossy()
                            );

                            return ExitCode::FAILURE;
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "error loading config script: {e} {}",
                        script_path.to_string_lossy()
                    );
                    return ExitCode::FAILURE;
                }
            }
        } else if let Some(script) = table.get("script") {
            let Some(script_value) = script.as_str() else {
                eprintln!(
                    "error getting config script from template {}: script is not a string",
                    template_file.to_string_lossy()
                );
                return ExitCode::FAILURE;
            };

            match template_environment.render_str(script_value, context!()) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!(
                        "error parsing config script jinja from template: {e} {}",
                        template_file.to_string_lossy()
                    );

                    return ExitCode::FAILURE;
                }
            }
        } else {
            template_missing_config_script = true;
            String::new()
        }
    } else {
        template_missing_config_script = true;
        String::new()
    };

    match config_toml.get("config") {
        Some(config) => {
            let Some(table) = config.as_table() else {
                eprintln!(
                    "error getting config script values from config {}: is not a table",
                    config_file.to_string_lossy()
                );
                return ExitCode::FAILURE;
            };

            if let Some(script) = table.get("script-file") {
                let Some(script_path) = script.as_str() else {
                    eprintln!(
                        "error getting config script file from config {}: script-file is not a string",
                        config_file.to_string_lossy()
                    );
                    return ExitCode::FAILURE;
                };

                let script_path = custom_scripts_dir.join(script_path.trim_start_matches('/'));
                match fs::read_to_string(&script_path) {
                    Ok(script_value) => {
                        if !template_missing_config_script {
                            config_script.push_str("\n\n");
                        }

                        match custom_environment.render_str(&script_value, context!()) {
                            Ok(x) => config_script.push_str(&x),
                            Err(e) => {
                                eprintln!(
                                    "error parsing config script jinja from config: {e} {}",
                                    script_path.to_string_lossy()
                                );

                                return ExitCode::FAILURE;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "error loading config script: {e} {}",
                            script_path.to_string_lossy()
                        );
                        return ExitCode::FAILURE;
                    }
                }
            } else if let Some(script) = table.get("script") {
                let Some(script_value) = script.as_str() else {
                    eprintln!(
                        "error getting config script from config {}: script is not a string",
                        config_file.to_string_lossy()
                    );
                    return ExitCode::FAILURE;
                };

                if !template_missing_config_script {
                    config_script.push_str("\n\n");
                }

                match custom_environment.render_str(script_value, context!()) {
                    Ok(x) => config_script.push_str(&x),
                    Err(e) => {
                        eprintln!(
                            "error parsing config script jinja from config: {e} {}",
                            config_file.to_string_lossy()
                        );

                        return ExitCode::FAILURE;
                    }
                }
            } else if template_missing_config_script {
                eprintln!(
                    "error missing config script values from config and template. Set config script values in atleast one"
                );
                return ExitCode::FAILURE;
            }
        }
        None => {
            if template_missing_config_script {
                eprintln!(
                    "error missing config script values from config and template. Set config script values in atleast one"
                );
                return ExitCode::FAILURE;
            }
        }
    }

    let install_script = if let Some(install) = config_toml.get("install") {
        let Some(table) = install.as_table() else {
            eprintln!(
                "error getting install script values from config {}: is not a table",
                config_file.to_string_lossy()
            );

            return ExitCode::FAILURE;
        };

        if let Some(script) = table.get("script-file") {
            let Some(script_path) = script.as_str() else {
                eprintln!(
                    "error getting install script file from config {}: script-file is not a string",
                    config_file.to_string_lossy()
                );

                return ExitCode::FAILURE;
            };

            let script_path = custom_scripts_dir.join(script_path.trim_start_matches('/'));
            match fs::read_to_string(&script_path) {
                Ok(script_value) => {
                    match custom_environment.render_str(&script_value, context!()) {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!(
                                "error parsing install script jinja from config: {e} {}",
                                config_file.to_string_lossy()
                            );

                            return ExitCode::FAILURE;
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "error loading install script: {e} {}",
                        script_path.to_string_lossy()
                    );

                    return ExitCode::FAILURE;
                }
            }
        } else if let Some(script) = table.get("script") {
            let Some(script_value) = script.as_str() else {
                eprintln!(
                    "error getting install script from config {}: script is not a string",
                    config_file.to_string_lossy()
                );

                return ExitCode::FAILURE;
            };

            match custom_environment.render_str(script_value, context!()) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!(
                        "error parsing install script jinja from config: {e} {}",
                        config_file.to_string_lossy()
                    );

                    return ExitCode::FAILURE;
                }
            }
        } else {
            eprintln!("error missing install script-file or script values from config");

            return ExitCode::FAILURE;
        }
    } else {
        eprintln!("error missing install script values from config");

        return ExitCode::FAILURE;
    };

    let files_toml: Array = match template_toml.get_mut("files") {
        Some(files) => {
            let Some(array) = files.as_array_mut() else {
                eprintln!(
                    "error getting files array from template {}: is not an array",
                    template_file.to_string_lossy()
                );

                return ExitCode::FAILURE;
            };

            for element in array.iter_mut() {
                let Some(table) = element.as_table_mut() else {
                    eprintln!(
                        "error getting files table from template {}: {}: is not a table is a {}",
                        template_file.to_string_lossy(),
                        element,
                        element.type_str()
                    );

                    return ExitCode::FAILURE;
                };

                if let Some(source) = table.get_mut("source") {
                    let Some(source_str) = source.as_str() else {
                        eprintln!(
                            "error getting files table from template {}: source is not a string: {}",
                            template_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    };

                    let source_path = files_dir.join(source_str.trim_start_matches('/'));

                    *source = Value::String(source_path.to_string_lossy().to_string());
                } else {
                    eprintln!(
                        "error getting files table from template {}: source is missing from: {}",
                        template_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(iso_path) = table.get("iso_path") {
                    if !iso_path.is_str() {
                        eprintln!(
                            "error getting files table from template {}: iso_path is not a string: {}",
                            template_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    }
                } else {
                    eprintln!(
                        "error getting files table from template {}: iso_path is missing from: {}",
                        template_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(install_path) = table.get("install_path") {
                    if !install_path.is_str() {
                        eprintln!(
                            "error getting files table from template {}: install_path is not a string: {}",
                            template_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    }
                } else {
                    eprintln!(
                        "error getting files table from template {}: install_path is missing from: {}",
                        template_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(early) = table.get("early")
                    && !early.is_bool()
                {
                    eprintln!(
                        "error getting files table from template {}: early is not a boolean: {}",
                        template_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
            }

            array.clone()
        }
        None => Array::new(),
    };
    let custom_files_toml: Array = match config_toml.get_mut("files") {
        Some(files) => {
            let Some(array) = files.as_array_mut() else {
                eprintln!(
                    "error getting files array from config {}: is not an array",
                    config_file.to_string_lossy()
                );

                return ExitCode::FAILURE;
            };

            for element in array.iter_mut() {
                let Some(table) = element.as_table_mut() else {
                    eprintln!(
                        "error getting files table from config {}: {}: is not a table is a {}",
                        config_file.to_string_lossy(),
                        element,
                        element.type_str()
                    );

                    return ExitCode::FAILURE;
                };

                if let Some(source) = table.get_mut("source") {
                    let Some(source_str) = source.as_str() else {
                        eprintln!(
                            "error getting files table from config {}: source is not a string: {}",
                            config_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    };

                    let source_path = custom_files_dir.join(source_str.trim_start_matches('/'));

                    *source = Value::String(source_path.to_string_lossy().to_string());
                } else {
                    eprintln!(
                        "error getting files table from config {}: source is missing from: {}",
                        config_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(iso_path) = table.get("iso_path") {
                    if !iso_path.is_str() {
                        eprintln!(
                            "error getting files table from config {}: iso_path is not a string: {}",
                            config_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    }
                } else {
                    eprintln!(
                        "error getting files table from config {}: iso_path is missing from: {}",
                        config_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(install_path) = table.get("install_path") {
                    if !install_path.is_str() {
                        eprintln!(
                            "error getting files table from config {}: install_path is not a string: {}",
                            config_file.to_string_lossy(),
                            element
                        );

                        return ExitCode::FAILURE;
                    }
                } else {
                    eprintln!(
                        "error getting files table from config {}: install_path is missing from: {}",
                        config_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
                if let Some(early) = table.get("early")
                    && !early.is_bool()
                {
                    eprintln!(
                        "error getting files table from config {}: early is not a boolean: {}",
                        config_file.to_string_lossy(),
                        element
                    );

                    return ExitCode::FAILURE;
                }
            }

            array.clone()
        }
        None => Array::new(),
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

    let command_result = Command::new("xorriso")
        .arg("-osirrox")
        .arg("on")
        .arg("-indev")
        .arg(iso_file)
        .arg("-chmod_r")
        .arg("u+rwx")
        .arg("/")
        .arg("--")
        .arg("-extract")
        .arg("/")
        .arg(&temp_dir)
        .arg("-rollback_end")
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error extracting iso: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error extracting iso: {e}");
            return ExitCode::FAILURE;
        }
    }

    let preseed_file_path = temp_dir.join("preseed.cfg");
    let mut preseed_file = match File::create_new(&preseed_file_path) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error creating preseed file: {e} {}",
                preseed_file_path.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = writeln!(preseed_file, "#_preseed_V1") {
        eprintln!(
            "error writing to preseed file: {e} {}",
            preseed_file_path.to_string_lossy()
        );
        return ExitCode::FAILURE;
    }

    let early_command_key = "d-i preseed/early_command string";
    let mut early_command_value = String::new();
    let late_command_key = "d-i preseed/late_command string";
    let mut late_command_value = String::from(concat!(
        "cp /cdrom/config.sh /target/bin; ",
        "cp /cdrom/install.sh /target/bin; ",
        "cp /cdrom/install.service /target/etc/systemd/system; ",
        "cp /cdrom/install.timer /target/etc/systemd/system; ",
        "chmod +x /bin/config.sh /bin/install.sh; ",
        "in-target /bin/config.sh; ",
        "in-target systemctl enable install.timer; ",
    ));

    for file_table in files_toml.iter().chain(custom_files_toml.iter()) {
        let table = file_table
            .as_table()
            .expect("files array should be comprised of tables");

        let source = table
            .get("source")
            .expect("file table should have checked source existence")
            .as_str()
            .expect("files table should have been checked to be string");
        let file_name = source
            .split('/')
            .next_back()
            .expect("splitting a string will always return an iterator with content");
        let iso_path = table
            .get("iso_path")
            .expect("file table should have checked iso_path existence")
            .as_str()
            .expect("files table should have been checked to be string");
        let temp_iso_path_dir = temp_dir.join(iso_path.trim_start_matches('/'));
        let temp_iso_path = temp_iso_path_dir.join(file_name);
        let install_path = table
            .get("install_path")
            .expect("file table should have checked install_path existence")
            .as_str()
            .expect("files table should have been checked to be string");

        if let Err(e) = fs::create_dir_all(&temp_iso_path_dir) {
            eprintln!(
                "error creating directories on iso temp: {source} to: {} {e}",
                temp_iso_path_dir.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }

        if let Err(e) = fs::copy(source, &temp_iso_path) {
            eprintln!(
                "error copying file: {source} to: {} {e}",
                temp_iso_path.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }

        if let Some(early_toml) = table.get("early")
            && let Some(early) = early_toml.as_bool()
            && early
        {
            early_command_value.push_str(
                format!(
                    "cp /cdrom/{}/{} /{}; ",
                    iso_path.trim_start_matches('/').trim_end_matches('/'),
                    file_name.trim_start_matches('/'),
                    install_path.trim_start_matches('/'),
                )
                .as_str(),
            );
        } else {
            late_command_value.push_str(
                format!(
                    "cp /cdrom/{}/{} /target/{}; ",
                    iso_path.trim_start_matches('/').trim_end_matches('/'),
                    file_name.trim_start_matches('/'),
                    install_path.trim_start_matches('/'),
                )
                .as_str(),
            );
        }
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

    for (key, value) in &preseed_toml {
        if let Err(e) = writeln!(
            preseed_file,
            "{key} {}",
            value
                .as_str()
                .expect("table should have been checked to be only strings")
        ) {
            eprintln!(
                "error writing to preseed file: {e} {}",
                preseed_file_path.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }
    }

    let config_script_path = temp_dir.join("config.sh");
    let mut config_script_file = match File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(&config_script_path)
    {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error creating config script: {e} {}",
                config_script_path.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = writeln!(config_script_file, "{config_script}") {
        eprintln!(
            "error writing to config script: {e} {}",
            config_script_path.to_string_lossy()
        );

        return ExitCode::FAILURE;
    }

    let install_script_path = temp_dir.join("install.sh");
    let mut install_script_file = match File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(&install_script_path)
    {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error creating install script: {e} {}",
                install_script_path.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = writeln!(install_script_file, "{install_script}") {
        eprintln!(
            "error writing to install script: {e} {}",
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
    let mut install_service_file = match File::create_new(&install_service_path) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error creating install service file: {e} {}",
                install_service_path.to_string_lossy()
            );

            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = write!(install_service_file, "{install_service_literal}") {
        eprintln!(
            "error writing to install service file: {e} {}",
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
    let mut install_timer_file = match File::create_new(&install_timer_path) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error creating install timer file: {e} {}",
                install_timer_path.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = write!(install_timer_file, "{install_timer_literal}") {
        eprintln!(
            "error writing to install timer file: {e} {}",
            install_timer_path.to_string_lossy()
        );
        return ExitCode::FAILURE;
    }

    // enable bios booting
    let command_result = Command::new("sed")
        .arg("-i")
        .arg("s/append/append auto=true priority=critical file=\\/cdrom\\/preseed.cfg/g")
        .arg(temp_dir.join("isolinux/txt.cfg"))
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error running sed: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error running sed: {e}");
            return ExitCode::FAILURE;
        }
    }

    let command_result = Command::new("sed")
        .arg("-i")
        .arg("s/timeout 0/timeout 50/g")
        .arg(temp_dir.join("isolinux/isolinux.cfg"))
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error running sed: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error running sed: {e}");
            return ExitCode::FAILURE;
        }
    }

    let command_result = Command::new("sed")
        .arg("-i")
        .arg("s/default vesamenu.c32/default install/g")
        .arg(temp_dir.join("isolinux/isolinux.cfg"))
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error running sed: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error running sed: {e}");
            return ExitCode::FAILURE;
        }
    }

    // enable uefi booting
    let grub_cfg_path = temp_dir.join("boot/grub/grub.cfg");
    let command_result = Command::new("sed")
        .arg("-i")
        .arg("s/---/auto=true priority=critical preseed\\/file=\\/cdrom\\/preseed.cfg ---/g")
        .arg(&grub_cfg_path)
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error running sed: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error running sed: {e}");
            return ExitCode::FAILURE;
        }
    }

    let mut file = match OpenOptions::new().append(true).open(&grub_cfg_path) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error loading grub config file: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = writeln!(
        file,
        "set default=1\n\
        set timeout=5"
    ) {
        eprintln!("error writing to grub config file: {e}");
        return ExitCode::FAILURE;
    }

    let command_result = Command::new("xorriso")
        .arg("-as")
        .arg("mkisofs")
        .arg("-r")
        .arg("-V")
        .arg("CUSTOM_DEBIAN")
        .arg("-o")
        .arg(&output_path)
        .arg("-isohybrid-mbr")
        .arg("/usr/lib/ISOLINUX/isohdpfx.bin")
        .arg("-b")
        .arg("isolinux/isolinux.bin")
        .arg("-c")
        .arg("isolinux/boot.cat")
        .arg("-boot-load-size")
        .arg("4")
        .arg("-boot-info-table")
        .arg("-no-emul-boot")
        .arg("-eltorito-alt-boot")
        .arg("-e")
        .arg("boot/grub/efi.img")
        .arg("-no-emul-boot")
        .arg("-isohybrid-gpt-basdat")
        .arg(&temp_dir)
        .output();

    match command_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = str::from_utf8(&output.stderr)
                    .expect("stderr should be utf8.")
                    .to_string();

                eprintln!("error creating iso: {stderr}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("error creating iso: {e}");
            return ExitCode::FAILURE;
        }
    }

    if let Err(e) = fs::remove_dir_all(&temp_dir) {
        eprintln!("error deleting temp directory: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
