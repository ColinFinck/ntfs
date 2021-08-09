#!/bin/bash
set -eu

if [ "`whoami`" != "root" ]; then
    echo Needs to be run as root!
    exit 1
fi

dd if=/dev/zero of=testfs1 bs=1k count=2048
mkntfs -c 512 -L mylabel -F testfs1

mkdir mnt
mount -t ntfs-3g -o loop testfs1 mnt
cd mnt

# Create a file with a specific modification time that we can check.
touch -m -t 202101011337 empty-file

# Create some zeroed files, as allocated and sparse files.
dd if=/dev/zero of=file-with-5-zeros bs=1 count=5
dd if=/dev/zero of=big-sparse-file skip=5M bs=1 count=1

# Create subdirectories of subdirectories.
mkdir -p subdir/subsubdir

# Create a file with some basic real content.
echo abcdef > subdir/subsubdir/file-with-6-letters

# Create so many directories that the filesystem needs an INDEX_ROOT and INDEX_ALLOCATION.
mkdir many_subdirs
cd many_subdirs
for i in {1..512}; do
    mkdir $i
done
cd ..

cd ..
umount mnt
rmdir mnt
