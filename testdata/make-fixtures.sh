#!/bin/sh
# Rebuilds the filesystem fixtures (macOS only):  ./testdata/make-fixtures.sh
#
# Each image holds a GPT partition with one filesystem, where:
#   /DCIM/IMG_0001.JPG                      deleted (photo 1 from test.img)
#   /Holiday Photos/Puppy and kitten.jpg    deleted, with its folder (photo 2)
#   /notes.txt                              still there
# Images are trimmed after the last used byte (and the backup GPT dropped)
# to keep the repo small; the parsers cope with truncated images.
set -eu
cd "$(dirname "$0")"
WORK=$(mktemp -d)
dd if=../test.img of="$WORK/IMG_0001.JPG" bs=1 skip=1024 count=94978 2>/dev/null
dd if=../test.img of="$WORK/Puppy and kitten.jpg" bs=1 skip=102400 count=42355 2>/dev/null
printf 'Shopping list: milk, bread\n' > "$WORK/notes.txt"

make() { # <ExFAT|MS-DOS> <size> <name>
    IMG="$WORK/$3"
    diskutil image create blank --size "$2" --format RAW --fs "$1" --volumeName RECLAIMER "$IMG" >/dev/null
    MNT=$(mktemp -d)
    diskutil image attach --mountPoint "$MNT" "$IMG" >/dev/null
    mkdir "$MNT/DCIM" "$MNT/Holiday Photos"
    cp -X "$WORK/IMG_0001.JPG" "$MNT/DCIM/"
    cp -X "$WORK/Puppy and kitten.jpg" "$MNT/Holiday Photos/"
    cp -X "$WORK/notes.txt" "$MNT/"
    sync
    rm "$MNT/DCIM/IMG_0001.JPG"
    rm -rf "$MNT/Holiday Photos"
    sync
    diskutil eject "$MNT" >/dev/null
    python3 -I -c '
import sys
d = open(sys.argv[1], "rb").read()
end = (len(d[:-33 * 512].rstrip(b"\0")) + 511) // 512 * 512
open(sys.argv[2], "wb").write(d[:end])' "$IMG" "$3"
    echo "made $3"
}

make MS-DOS 4m  fat12.img   # macOS picks FAT12 at this size
make MS-DOS 40m fat16.img   # ... and FAT16 here
make ExFAT  4m  exfat.img
# FAT32 needs a much larger volume; tests build one in memory instead.
