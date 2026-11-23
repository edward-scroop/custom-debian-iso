// SPDX-FileCopyrightText: Copyright 2026 Edward Scroop <edward.scroop@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    fs::{create_dir, remove_dir_all},
    io::{Error, ErrorKind},
    path::Path,
};

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

pub fn check_if_file_exists(file: &Path) -> Result<(), String> {
    check_if_exists(file, &CheckType::File)
}

pub fn check_if_dir_exists(dir: &Path) -> Result<(), String> {
    check_if_exists(dir, &CheckType::Dir)
}

pub fn delete_dir(dir: &Path) -> Result<(), Error> {
    if let Err(e) = remove_dir_all(dir)
        && e.kind() != ErrorKind::NotFound
    {
        return Err(e);
    }

    Ok(())
}

pub fn prepare_temp_and_output_dir(temp_dir: &Path, output_dir: &Path) -> Result<(), String> {
    if let Err(e) = delete_dir(temp_dir) {
        return Err(format!(
            "error deleting temp directory: {} {}",
            e,
            temp_dir.to_string_lossy()
        ));
    }

    if let Err(e) = create_dir(temp_dir)
        && e.kind() != ErrorKind::AlreadyExists
    {
        return Err(format!(
            "error creating temp directory: {e} {}",
            temp_dir.to_string_lossy()
        ));
    }

    if let Err(e) = create_dir(output_dir)
        && e.kind() != ErrorKind::AlreadyExists
    {
        return Err(format!(
            "error creating output directory: {e} {}",
            temp_dir.to_string_lossy()
        ));
    }

    Ok(())
}
