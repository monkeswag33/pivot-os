#! /bin/bash
set -e

IMG=$1
FAT=$2
IMG_SIZE=$3

rm -f $IMG
truncate -s $IMG_SIZE $IMG
parted $IMG -m -s -a min mklabel gpt mkpart EFI FAT32 2048s 100%
dd if=$FAT of=$IMG seek=2048 conv=notrunc