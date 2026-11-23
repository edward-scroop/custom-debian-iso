// SPDX-FileCopyrightText: Copyright 2026 Edward Scroop <edward.scroop@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
    process::Command,
};

pub fn extract_iso(iso_file: &Path, output_dir: &Path) -> io::Result<()> {
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
        .arg(output_dir)
        .arg("-rollback_end")
        .output()?;

    if !command_result.status.success() {
        let stderr = str::from_utf8(&command_result.stderr)
            .expect("stderr should be utf8.")
            .to_string();

        return Err(io::Error::other(stderr));
    }

    Ok(())
}

pub fn create_iso(output_file: &Path, temp_dir: &Path) -> io::Result<()> {
    let command_result = Command::new("xorriso")
        .arg("-as")
        .arg("mkisofs")
        .arg("-r")
        .arg("-V")
        .arg("CUSTOM_DEBIAN")
        .arg("-o")
        .arg(output_file)
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
        .arg(temp_dir)
        .output()?;

    if !command_result.status.success() {
        let stderr = str::from_utf8(&command_result.stderr)
            .expect("stderr should be utf8.")
            .to_string();

        return Err(io::Error::other(stderr));
    }

    Ok(())
}

pub fn enable_bios_booting(temp_dir: &Path) -> Result<(), String> {
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

                return Err(format!("sed error: {stderr}"));
            }
        }
        Err(e) => {
            return Err(format!("error running sed: {e}"));
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

                return Err(format!("sed error: {stderr}"));
            }
        }
        Err(e) => {
            return Err(format!("error running sed: {e}"));
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

                return Err(format!("sed error: {stderr}"));
            }
        }
        Err(e) => {
            return Err(format!("error running sed: {e}"));
        }
    }

    Ok(())
}

pub fn enable_uefi_booting(temp_dir: &Path) -> Result<(), String> {
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

                return Err(format!("sed error: {stderr}"));
            }
        }
        Err(e) => {
            return Err(format!("error running sed: {e}"));
        }
    }

    let mut file = match OpenOptions::new().append(true).open(&grub_cfg_path) {
        Ok(x) => x,
        Err(e) => {
            return Err(format!("error loading grub config file: {e}"));
        }
    };

    if let Err(e) = writeln!(
        file,
        "set default=1\n\
        set timeout=5"
    ) {
        return Err(format!("error writing to grub config file: {e}"));
    }

    Ok(())
}
