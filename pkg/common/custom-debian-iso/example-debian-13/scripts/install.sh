#!/bin/bash
set -euxo pipefail
script=$(realpath "$0")

# Run bash code to configure install after rebooting

systemctl disable install.timer
rm -f /etc/systemd/system/install.timer /etc/systemd/system/install.service
rm -f "$script"
