#!/bin/bash

fs_dir=/tmp/fusefs

cargo run & sleep 2

cd $fs_dir

echo "Creating directories..."
mkdir dir1
mkdir dir2

echo "Creating files..."
echo "file0" > file0.txt
echo "file1" > dir1/file1.txt
echo "file2" > dir2/file2.txt

echo "Updating files..."
echo "file0 updated" > file0.txt
echo "file1 append" >> dir1/file1.txt

echo "Displaying files..."
tree

echo "Removing files..."
rm file0.txt
rm -rf dir1
rm -rf dir2

echo "Unmounting..."
cd ..
fusermount -u $fs_dir