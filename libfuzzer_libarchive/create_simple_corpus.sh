#!/bin/bash

# Create folder structure
mkdir -p test_data/subfolder

# Create some files
echo "Hello World" > test_data/file1.txt
echo "Test content" > test_data/file2.txt
echo "Another file" > test_data/readme.md
echo "Subfolder content" > test_data/subfolder/data.txt
echo "More data" > test_data/subfolder/info.log

# Create corpus directory
mkdir -p simple_corpus

# Create different archive formats
cd test_data
tar -cf ../simple_corpus/test.tar .
tar -czf ../simple_corpus/test.tar.gz .
zip -r ../simple_corpus/test.zip .
cd ..

# Clean up
rm -rf test_data

echo "Created archives in libfuzzer_libarchive/simple_corpus/"