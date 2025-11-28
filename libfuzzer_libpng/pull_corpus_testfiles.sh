#!/bin/bash

# Create corpus directory
rm -rf corpus
mkdir -p corpus
cd corpus

echo "Fetching file list from GitHub..."

# Get the list of all PNG files from the GitHub API
REPO="pnggroup/libpng"
PATH_IN_REPO="contrib/testpngs"
BRANCH="libpng18"

# Get all files using GitHub API
curl -s "https://api.github.com/repos/${REPO}/contents/${PATH_IN_REPO}?ref=${BRANCH}" | \
  grep '"name"' | \
  grep '\.png"' | \
  cut -d'"' -f4 > png_list.txt

# Count files
TOTAL=$(wc -l < png_list.txt)
echo "Found $TOTAL PNG files to download"

# Download each file
COUNT=0
while IFS= read -r filename; do
    COUNT=$((COUNT + 1))
    echo "[$COUNT/$TOTAL] Downloading $filename..."
    wget -q "https://github.com/${REPO}/raw/${BRANCH}/${PATH_IN_REPO}/${filename}"
done < png_list.txt

# Clean up
rm png_list.txt

echo "✓ Downloaded $(ls -1 *.png 2>/dev/null | wc -l) PNG files to corpus/"
cd ..