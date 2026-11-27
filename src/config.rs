// SPDX-FileCopyrightText: Copyright 2026 Edward Scroop <edward.scroop@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use minijinja::{Environment, context};
use std::{
    fs::{File, copy, create_dir_all, read_to_string},
    io::{self, Error, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
use toml::{Table, Value, value::Array};

pub fn toml_get_string(key_name: &str, toml_table: &mut Table) -> Result<String, String> {
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

pub fn load_toml(config_file: &Path) -> io::Result<Table> {
    return toml::from_str(&read_to_string(config_file)?).map_err(Error::other);
}

pub fn get_preseed_from_toml(
    template_preseed: Option<&Value>,
    config_preseed: Option<&Value>,
    template_file: &Path,
    config_file: &Path,
) -> Result<Table, String> {
    let mut preseed_toml = match parse_preseed_toml_table(template_preseed) {
        Ok(x) => x,
        Err(e) => {
            return Err(format!(
                "error getting preseed values from template {}: {e}",
                template_file.to_string_lossy()
            ));
        }
    };

    if let Some(x) = config_preseed
        && let Err(e) = extend_preseed_toml_table(&mut preseed_toml, x)
    {
        return Err(format!(
            "error getting preseed values from config {}: {e}",
            config_file.to_string_lossy()
        ));
    }

    Ok(preseed_toml)
}

fn parse_preseed_toml_table(toml_value: Option<&Value>) -> Result<Table, String> {
    let Some(toml) = toml_value else {
        return Ok(Table::new());
    };

    if let Some(table) = toml.as_table() {
        for (key, value) in table {
            if !value.is_str() {
                return Err(format!("error key: {key} value: {value} is not a string"));
            }
        }

        Ok(table.clone())
    } else {
        Err(format!("error value {:#?}: is not a table", toml_value))
    }
}

fn extend_preseed_toml_table(target_table: &mut Table, toml: &Value) -> Result<(), String> {
    if let Some(table) = toml.as_table().cloned() {
        for (key, value) in &table {
            if !value.is_str() {
                return Err(format!("error key: {key} value: {value} is not a string"));
            }
        }

        target_table.extend(table);
        Ok(())
    } else {
        Err(format!("error value {:#?}: is not a table", toml))
    }
}

pub fn get_template_minijinja_environment(scripts_dir: &Path) -> Environment<'static> {
    let mut template_environment = Environment::new();

    template_environment.set_auto_reload(false);
    template_environment.set_loader(minijinja::path_loader(scripts_dir));

    template_environment
}

pub fn get_custom_minijinja_environment(
    work_dir: &Path,
    custom_iso_name: String,
) -> Environment<'static> {
    let mut custom_environment = Environment::new();

    custom_environment.set_auto_reload(false);
    custom_environment.set_loader(minijinja::path_loader(work_dir));
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

    custom_environment
}

pub fn get_install_script_from_toml(
    custom_install_toml: Option<&Value>,
    scripts_dir: &Path,
    custom_environment: &Environment,
) -> Result<String, String> {
    let custom_config_script =
        match get_script(custom_install_toml, &scripts_dir, &custom_environment) {
            Ok(x) => x,
            Err(e) => return Err(e),
        };

    if let Some(custom) = custom_config_script {
        return Ok(custom);
    } else {
        return Err(String::from(
            "missing install script values from config. Set install script values in config",
        ));
    };
}

pub fn get_config_script_from_toml(
    template_config_toml: Option<&Value>,
    custom_config_toml: Option<&Value>,
    scripts_dir: &Path,
    template_environment: &Environment,
    custom_environment: &Environment,
) -> Result<String, String> {
    let template_config_script =
        match get_script(template_config_toml, &scripts_dir, &template_environment) {
            Ok(x) => x,
            Err(e) => return Err(e),
        };

    let custom_config_script =
        match get_script(custom_config_toml, &scripts_dir, &custom_environment) {
            Ok(x) => x,
            Err(e) => return Err(e),
        };

    if let Some(custom) = custom_config_script {
        let mut string = if let Some(mut template) = template_config_script {
            template.push_str("\n\n");
            template
        } else {
            String::new()
        };

        string.push_str(&custom);

        return Ok(string);
    } else if template_config_script.is_none() {
        return Err(String::from(
            "missing config script values from config and template. Set config script values in atleast config",
        ));
    } else {
        return Err(String::from(
            "missing config script values from config. Set config script values in atleast config",
        ));
    };
}

fn get_script(
    config_toml: Option<&Value>,
    scripts_dir: &Path,
    template_environment: &Environment,
) -> Result<Option<String>, String> {
    let Some(config) = config_toml else {
        return Err(String::from("config script table missing"));
    };

    let Some(table) = config.as_table() else {
        return Err(format!(
            "config script table is not a table: is {}",
            config.type_str()
        ));
    };

    if let Some(script) = table.get("script-file") {
        let Some(script_path) = script.as_str() else {
            return Err(format!(
                "config script key: script-file value: {script} is not a string, is: {}",
                script.type_str()
            ));
        };

        let script_path = scripts_dir.join(script_path.trim_start_matches('/'));
        match read_to_string(&script_path) {
            Ok(script_value) => match template_environment.render_str(&script_value, context!()) {
                Ok(x) => Ok(Some(x)),
                Err(e) => {
                    return Err(format!(
                        "error parsing config script jinja from template: {e}"
                    ));
                }
            },
            Err(e) => {
                return Err(format!("error loading config script: {e}"));
            }
        }
    } else if let Some(script) = table.get("script") {
        let Some(script_value) = script.as_str() else {
            return Err(format!(
                "config script key: script value: {script} is not a string, is: {}",
                script.type_str()
            ));
        };

        match template_environment.render_str(script_value, context!()) {
            Ok(x) => Ok(Some(x)),
            Err(e) => {
                return Err(format!(
                    "error parsing config script jinja from template: {e}"
                ));
            }
        }
    } else {
        Ok(None)
    }
}

pub fn write_string_to_file(
    file_path: &Path,
    content: &str,
    executable: bool,
) -> Result<(), String> {
    let mut options = File::options();

    options.read(true).write(true).create_new(true);
    if executable {
        options.mode(0o755);
    }

    let mut file = match options.open(file_path) {
        Ok(x) => x,
        Err(e) => {
            return Err(format!("{e}"));
        }
    };

    if let Err(e) = writeln!(file, "{content}") {
        return Err(format!("{e}"));
    }

    Ok(())
}

pub fn process_files_toml(files: Option<&mut Value>, files_dir: &Path) -> Result<Array, String> {
    let Some(files) = files else {
        return Ok(Array::new());
    };

    let Some(array) = files.as_array_mut() else {
        return Err(format!("files is not an array, is: {}", files.type_str()));
    };

    for element in array.iter_mut() {
        let element_string = element.to_string();
        let Some(table) = element.as_table_mut() else {
            return Err(format!(
                "files: {} is not an array, is: {}",
                element,
                element.type_str()
            ));
        };
        let Some(source) = table.get_mut("source") else {
            return Err(format!("source is missing from {}", element));
        };
        let Some(source_str) = source.as_str() else {
            return Err(format!(
                "source from {}: is not a string, is: {}",
                element_string,
                source.type_str()
            ));
        };
        let source_path = files_dir.join(source_str.trim_start_matches('/'));

        *source = Value::String(source_path.to_string_lossy().to_string());

        let Some(iso_path) = table.get("iso_path") else {
            return Err(format!("iso_path is missing from {}", element_string));
        };
        let Some(install_path) = table.get("install_path") else {
            return Err(format!("install_path is missing from: {}", element_string,));
        };

        if !iso_path.is_str() {
            return Err(format!(
                "iso_path from {} is not a string, is: {}",
                element_string,
                iso_path.type_str()
            ));
        }

        if !install_path.is_str() {
            return Err(format!(
                "install_path from {} is not a string, is: {}",
                element_string,
                install_path.type_str()
            ));
        }

        if let Some(early) = table.get("early")
            && !early.is_bool()
        {
            return Err(format!(
                "early from {} is not a boolean, is: {}",
                element_string,
                early.type_str()
            ));
        }
    }

    Ok(array.clone())
}

pub fn process_files_preseed<'a>(
    files: impl Iterator<Item = &'a Value>,
    early_command_value: &mut String,
    late_command_value: &mut String,
    temp_dir: &Path,
) -> Result<(), String> {
    for file_table in files {
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

        if let Err(e) = create_dir_all(&temp_iso_path_dir) {
            return Err(format!(
                "error creating directories on iso temp: {source} to: {} {e}",
                temp_iso_path_dir.to_string_lossy()
            ));
        }

        if let Err(e) = copy(source, &temp_iso_path) {
            return Err(format!(
                "error copying file: {source} to: {} {e}",
                temp_iso_path.to_string_lossy()
            ));
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

    Ok(())
}
