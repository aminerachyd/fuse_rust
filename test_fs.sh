#!/bin/bash

fs_dir=/tmp/fusefs

cargo run& 
echo "Waiting for 10 seconds for Fuse server to start..."
sleep 10

cd $fs_dir

echo "Creating directories..."
mkdir dir1
mkdir dir2
if [ -d dir1 ] && [ -d dir2 ]; then
    echo "Directories created successfully"
else
    echo "Directories not created"
    kill $pid
    exit 1
fi

echo "Creating files..."
echo "file0" > file0.txt
echo "file1" > dir1/file1.txt
echo "file2" > dir2/file2.txt
if [ -f file0.txt ] && [ -f dir1/file1.txt ] && [ -f dir2/file2.txt ]; then
    echo "Files created successfully"
else
    echo "Files not created"
    kill $pid
    exit 1
fi

echo "Updating files..."
echo "file0 updated" > file0.txt
echo "file1 append" >> dir1/file1.txt
if [ "$(cat file0.txt)" == "file0 updated" ]; then
    echo "Files updated successfully"
else
    echo "Files not updated"
    kill $pid
    exit 1
fi

echo "Displaying files..."
tree

echo "Removing files..."
rm file0.txt
rm -rf dir1
rm -rf dir2
if [ ! -f file0.txt ] && [ ! -d dir1 ] && [ ! -d dir2 ]; then
    echo "Files removed successfully"
else
    echo "Files not removed"
    kill $pid
    exit 1
fi

echo "Unmounting..."
cd ..
fusermount -u $fs_dir