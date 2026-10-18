#!/bin/sh
# Turns the raw recordings in website/.media-raw/ (written by packages/ui/e2e/website-media.mjs) into the
# published media in website/assets/media/: WebM (VP9) and MP4 (H.264) clips of at most 15 s, a JPEG poster
# per clip, and JPEG screenshots (2x). Needs ffmpeg. Each clip should stay under about 2 MB.
set -eu
cd "$(dirname "$0")"
RAW=.media-raw
OUT=assets/media
mkdir -p "$OUT"
command -v ffmpeg >/dev/null || { echo "ffmpeg not found (brew install ffmpeg)"; exit 1; }
FPS=${FPS:-25}
for json in "$RAW"/clip-*.json; do
  id=$(basename "$json" .json)
  start=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['start'])" "$json")
  dur=$(python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(min(15.0, round(d['end']-d['start'], 2)))" "$json")
  vf="fps=$FPS,scale=1280:800:flags=lanczos"
  ffmpeg -v error -y -ss "$start" -t "$dur" -i "$RAW/$id.webm" -vf "$vf" -an \
    -c:v libvpx-vp9 -b:v 0 -crf 33 -row-mt 1 -deadline good -cpu-used 2 "$OUT/$id.webm"
  ffmpeg -v error -y -ss "$start" -t "$dur" -i "$RAW/$id.webm" -vf "$vf" -an \
    -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p -profile:v high -movflags +faststart "$OUT/$id.mp4"
  # poster: the last second of the clip, where the shown state is complete
  pos=$(python3 -c "print(max(0, $dur - 1.0))")
  ffmpeg -v error -y -ss "$pos" -i "$OUT/$id.mp4" -frames:v 1 -q:v 4 "$OUT/$id.jpg"
  printf '%s: %ss, webm %s KB, mp4 %s KB\n' "$id" "$dur" $(( $(wc -c < "$OUT/$id.webm") / 1024 )) $(( $(wc -c < "$OUT/$id.mp4") / 1024 ))
done
for png in "$RAW"/shot-*.png; do
  id=$(basename "$png" .png)
  ffmpeg -v error -y -i "$png" -q:v 3 "$OUT/$id.jpg"
  printf '%s: %s KB\n' "$id" $(( $(wc -c < "$OUT/$id.jpg") / 1024 ))
done
