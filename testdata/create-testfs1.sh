#!/bin/bash
set -eu

if [ "`whoami`" != "root" ]; then
    echo Needs to be run as root!
    exit 1
fi

dd if=/dev/zero of=testfs1 bs=1k count=1025
mkntfs -c 512 -L mylabel -F testfs1

mkdir mnt
mount -t ntfs-3g -o loop testfs1 mnt
cd mnt

touch -m -t 202101011337 empty-file
dd if=/dev/zero of=file-with-5-zeros bs=1 count=5
dd if=/dev/zero of=big-sparse-file skip=5M bs=1 count=1

mkdir -p subdir/subsubdir
echo abcdef > subdir/subsubdir/file-with-6-letters

cd ..
umount mnt
rmdir mnt
