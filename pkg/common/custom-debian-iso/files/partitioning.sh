#!/bin/sh

echo "running partition.sh" >> /var/log/syslog

list-devices disk | grep -oE "[s|v]d[a-z]*" | while read disk; do
    echo "$disk $(( ($(blockdev --getsize64 /dev/$disk) + 1) / (1024 * 1024 * 1024) ))" >> /tmp/disks
done

sed -i "/ 0$/d" /tmp/disks
cat /tmp/disks | sort -gk 2 | head -n 2 >> /tmp/sorted
disk1="/dev/$(cat /tmp/sorted | head -n 1 | grep -oE '[s|v]d([a-z]){1,2}')"

dd if=/dev/zero of=$disk1 bs=512 count=34

cat << EOF >> /tmp/partition.cfg
d-i partman-auto/disk string $disk1
d-i partman-auto/method string regular
d-i partman-auto-lvm/guided_size string max
d-i partman-lvm/device_remove_lvm boolean true
d-i partman-md/device_remove_md boolean true
d-i partman-lvm/confirm boolean true
d-i partman-lvm/confirm_nooverwrite boolean true
d-i partman-auto/choose_recipe select custom-partitioning
d-i partman-basicfilesystems/no_swap boolean false
d-i partman-md/confirm boolean true
d-i partman-md/confirm_nooverwrite boolean true
d-i mdadm/boot_degraded boolean true
d-i partman/choose_partition select finish
d-i partman/confirm boolean true
d-i partman/confirm_nooverwrite boolean true
d-i partman-efi/non_efi_system boolean false
d-i partman-partitioning/confirm_write_new_label true
d-i partman-partitioning/choose_label select gpt
d-i partman-partitioning/default_label string gpt

d-i partman/mount_style select label

### Boot loader installation
d-i grub-installer/only_debian boolean true
d-i grub-installer/with_other_os boolean true
d-i grub-pc/install_devices multiselect $disk1
d-i grub-installer/bootdev string $disk1
EOF

cat << EOF >> /tmp/partition.cfg
d-i partman-auto/expert_recipe string	      \\
		custom-partitioning ::			\\
		1075 1075 1075 free			\\
                \$iflabel{ gpt}                 \\
                \$reusemethod{ }                \\
		    method{ efi }			      \\
		    format{ }			      \\
		    mountpoint{ /boot/efi }		\\
		.						\\
		1075 10000 -1 btrfs			\\
	          method{ format }			\\
		    format{ }				\\
		    use_filesystem{ }			\\
		    filesystem{ btrfs }		      \\
		    mountpoint{ / }			\\
		    device{ $disk1 }			\\
		    options/noatime{ noatime }      \\
		.						\\
		1075 10750 21500 linux-swap		\\
		    method{ swap }			\\
		    format{ }				\\
		.
EOF

debconf-set-selections /tmp/partition.cfg
