#!/usr/bin/env bash

set -e

BUILD_TYPE=release
MOUNT_POINT="/home/troy/mnt"
TARGET="thumbv6m-none-eabi"
BINARY_NAME=${1:-plane}

# Build the project
echo "Building project..."
cargo build --bin $BINARY_NAME --${BUILD_TYPE}

# Locate the Pico device (128MB)
echo "Searching for Pico device..."
DEVICE=$(lsblk -o NAME,SIZE -bpn | grep "134217728" | awk '{print $1}' | head -n1)

if [ -z "$DEVICE" ]; then
    echo "Raspberry Pi Pico device not found! Make sure it's connected in boot mode."
    exit 1
fi

DEVICE_PARTITION=${DEVICE}1

# Mount the Pico device
echo "Mounting Pico device $DEVICE_PARTITION to $MOUNT_POINT..."
sudo mount -o uid=$(id -u),gid=$(id -g) $DEVICE_PARTITION $MOUNT_POINT

# Run the upload command
echo "Uploading firmware to Pico..."
elf2uf2-rs -d target/${TARGET}/${BUILD_TYPE}/$BINARY_NAME $MOUNT_POINT

# Unmount after upload
sync
sudo umount $MOUNT_POINT

echo "Upload complete!"

