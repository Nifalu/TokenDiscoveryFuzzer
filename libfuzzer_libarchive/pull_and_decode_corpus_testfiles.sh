#!/bin/bash

mkdir -p corpus && cd corpus

curl -s https://api.github.com/repos/libarchive/libarchive/contents/libarchive/test \
  | grep -oP '"name": "\K[^"]*\.uu(?=")' \
  | while read file; do
    echo "Downloading $file"
    curl -sO "https://raw.githubusercontent.com/libarchive/libarchive/master/libarchive/test/$file"
done

# Decode all
for file in *.uu; do
    uudecode "$file" && rm "$file"
done

cd ..
echo "Done. Files in ./corpus/"
