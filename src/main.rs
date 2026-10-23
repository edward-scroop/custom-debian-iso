use clap::Parser;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use toml::{Table, Value};

enum CheckType {
    File,
    Dir,
}

fn check_if_exists(file: &Path, check_type: CheckType) -> Result<(), String> {
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
    };
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
        } else {
            return Err(format!("Error {} is a {}", file.to_string_lossy(), {
                match check_type {
                    CheckType::File => "file",
                    CheckType::Dir => "directory",
                }
            }));
        }
    }

    Ok(())
}

fn check_if_file_exists(file: &Path) -> Result<(), String> {
    check_if_exists(file, CheckType::File)
}

fn check_if_dir_exists(dir: &Path) -> Result<(), String> {
    check_if_exists(dir, CheckType::Dir)
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
        /// Path of the custom iso config directory. Is relative to the work_dir. Will search for a
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

    let isos_dir = work_dir.join("isos");
    let scripts_dir = work_dir.join("scripts");
    let templates_dir = work_dir.join("templates");
    let files_dir = work_dir.join("files");
    let temp_dir = work_dir.join(".tmp");
    let output_dir = work_dir.join("output");
    let custom_dir = work_dir.join(args.custom_dir);
    let custom_iso_name = custom_dir
        .file_name()
        .expect("custom dir should a directory")
        .to_string_lossy()
        .to_string();

    for dir in [
        &work_dir,
        &isos_dir,
        &scripts_dir,
        &templates_dir,
        &files_dir,
        &custom_dir,
    ] {
        if let Err(e) = check_if_dir_exists(dir) {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }

    if let Err(e) = fs::remove_dir_all(&temp_dir) {
        if e.kind() != io::ErrorKind::NotFound {
            eprintln!(
                "error deleting temp directory: {e} {}",
                &temp_dir.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = fs::create_dir(&temp_dir) {
        eprintln!(
            "error creating temp directory: {e} {}",
            &temp_dir.to_string_lossy()
        );
        return ExitCode::FAILURE;
    };

    if let Err(e) = fs::create_dir(&output_dir) {
        if e.kind() != io::ErrorKind::AlreadyExists {
            eprintln!(
                "error creating output directory: {e} {}",
                &output_dir.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };

    let config_file = custom_dir.join("config.toml");
    let config_string = match fs::read_to_string(&config_file) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "error getting config: {e} {}",
                &config_file.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };
    let mut config_toml: Table = match toml::from_str(&config_string) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error getting config: {e}",);
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
                &template_file.to_string_lossy()
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
    };

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
    };

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
    };

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
    };

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
    };

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

    let mut output_path = output_dir.join(custom_iso_name);
    output_path.add_extension("iso");

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
    };

    if let Err(e) = fs::remove_dir_all(&temp_dir) {
        eprintln!("error deleting temp directory: {e}");
        return ExitCode::FAILURE;
    };

    return ExitCode::SUCCESS;
}
