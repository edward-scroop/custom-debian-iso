# custom-debian-iso

`custom-debian-iso` is a Rust program to create automated Debian ISOs with preseed files and custom scripts.
The custom iso will be able to boot on both BIOS and UEFI systems.

## Installation

You can download the binary or install it from source using `cargo install`.

### Debian package

First, download the `.deb` package from [the GitHub releases page][releases].

Then use `dpkg` to install the package:

```bash
sudo dpkg -i /path/custom-debian-iso.deb
```

Or use `apt` to install the package:

```bash
sudo apt-get install /path/custom-debian-iso.deb
# or
sudo apt install /path/custom-debian-iso.deb
```

### Cargo install

First, follow the [Rust docs][rust_docs] on how to install Cargo.  
Second, download the source code from [GitHub][repo]. Then, from inside the source directory, run:

```bash
cargo install --path .
```

## Usage

When installed from a `.deb` package, an example config directory is located at `/etc/custom-debian-iso/`. You can also view the example directory on [GitHub][example_directory].  
In the prepared directory, you must download an ISO into the `isos` directory, set the template to use that ISO, and set the custom ISO to use that template.  
Then you can run:

```bash
create-custom-iso <CUSTOM_DIR> [WORK_DIR]
```

Where `WORK_DIR` is the prepared directory (by default it is the current working directory; otherwise it will search for the provided path), and `CUSTOM_DIR` is the path relative to `WORK_DIR` for your custom ISO config directory.

## Work directory

The directory must contain these sub-directories:

- `isos`
- `templates`
- `files`
- `scripts`

### ISOs

This is where you must store ISOs for the templates to use.

### Templates

This is where you must store templates that custom ISOs can inherit from. Each template must be a TOML file with certain options.

### Files

This is where you must store files that templates can copy onto the output ISO and onto the installed system.

## Custom directory

The directory must contain a TOML file called `config.toml` and these sub-directories:

- `files`
- `scripts`

### Files

This is where you must store files that custom ISOs can copy onto the output ISO and onto the install.

## TOML config format

### Shared

Both configs can take a table named `preseed`, with the keys being the preseed `<owner> <question name> <question type>` and the value being the preseed value.  
They will be reproduced literally with spaces between, so see the [Debian config for preseed format][preseed_format].  
Any preseed key pair defined in the template can be overwritten in the custom ISO config by using the same key.

Example:

```toml
[preseed]
'd-i debian-installer/locale string' = 'en_US.UTF-8'
```

Both configs can take a table named `config` with a `script` and `script-file` key.  
The `script-file` key is a path to the script file relative to the script directory, and the `script` key is just the script content verbatim.  
A defined `script-file` will take priority over a defined `script`.  
The template does not need to include a `config`, but the custom ISO config must. The custom ISO config script will be appended to the template config script, if defined.  
See the [scripts sub-directory](#scripts-sub-directory) for important info.

Example:

```toml
[config]
script-file = "config.sh"
script = """
#!/bin/bash
echo "script"
"""
```

Both configs can take an array of tables named `files`, with the keys `source`, `iso_path`, and `install_path`.  
Key pair overwriting will not occur.

Example:

```toml
files = [
    { source = "example.txt", iso_path = "/", install_path = "/path" }
]
```

### Custom ISO

The custom ISO config must contain a root-level key of `template`, which must be a string containing the path of the template file to use relative to the template directory.

Example:

```toml
template = "base-debian-13.toml"
```

The custom ISO config can take a table named `install` with a `script` and `script-file` key.  
The `script-file` key is a path to the script file relative to the script directory, and the `script` key is just the script content verbatim.  
A defined `script-file` will take priority over a defined `script`.  
An install script must be defined.  
See the [scripts sub-directory](#scripts-sub-directory) for more important info.

Example:

```toml
[install]
script-file = "install.sh"
script = """
#!/bin/bash
echo "install"
"""
```

### Template

The template must contain a root-level key of `iso`, which must be a string containing the relative path of the ISO in the `isos` directory to use.

Example:

```toml
iso = "debian-13.7.iso"
```

## Scripts sub-directory

This is where you must store scripts that templates or custom ISOs can include.  
The scripts should be POSIX shell or Bash, depending on when and where they are run.

Scripts can also include other script fragments through a subset of Jinja syntax.  
Currently only `{% include %}` is supported, and the path must be relative to the scripts directory.  
For scripts located under custom ISO directories, any path starting with `shared` will include scripts from the work directory `scripts`.

The config script will run after preseed is finished and will have access to the installed system.  
You will be able to run most normal Debian things.  
You should configure settings such as IP addresses, SSH public keys, or storage settings in this script.

The install script will be set to run after boot, and this is where you should run code that can't be done before reboot or setting up things like Ansible.

## License

This project is licensed under the [GPLv3 or later license][license].

[repo]: https://github.com/edward-scroop/custom-debian-iso
[rust_docs]: https://rust-lang.org/tools/install
[example_directory]: https://github.com/edward-scroop/custom-debian-iso/tree/master/pkg/common/custom-debian-iso
[preseed_format]: https://www.debian.org/releases/stable/amd64/apbs04.en.html
[releases]: https://github.com/edward-scroop/custom-debian-iso/releases
[license]: https://github.com/edward-scroop/custom-debian-iso/blob/master/LICENSE.md
