# Recording the README video

[Current reel](https://github.com/user-attachments/assets/507e1db3-4e98-41e8-97d9-dbd75ad62fb8).
The recorder drives the visible viewer through its app control channel and
captures the real framebuffer. The encoder adds a visible pointer/click feedback,
captions and chapters. Retained footage keeps its original timing; scene cuts
remove setup waits rather than accelerating animation or interaction.

## Record, encode and check

Use a graphical desktop with a visible viewer window and localhost access.
Install Pillow and ReportLab for Python, and FFmpeg with `libx264`. Run from the
repository root, using fresh output directories:

```sh
cargo build --release -p pdfoxium-viewer
python3 scripts/record-viewer-showcase.py --reel --rehearse --output target/viewer-reel/rehearsal
python3 scripts/record-viewer-showcase.py --reel --output target/viewer-reel/take
python3 scripts/encode-viewer-showcase.py --input target/viewer-reel/take --output target/viewer-reel/viewer-showcase.mp4 --max-duration 70
python3 scripts/check-viewer-showcase.py --video target/viewer-reel/viewer-showcase.mp4 --take target/viewer-reel/take --output target/viewer-reel/validation --max-duration 70
```

`--rehearse` checks the storyboard quickly and keeps chapter stills.
`--max-duration` rejects an overlong result; it does not speed it up. Machine-
dependent waits may require adjusting cuts/storyboard. Omit `--reel` for the
longer walkthrough. `join-viewer-showcase.py` can combine takes at a chapter
boundary; see its `--help` for prefix/tail options.

Inspect the resulting video and chapter contact sheets. The validator checks
readability/chapters, visible cursor, successful app calls, animation play/pause,
thumbnail stability and visible map-layer removal/restoration. The layers segment
briefly hides roads and resets the map through the dropdown. This is a feature
reel, not a latency benchmark.
Fixtures are embedded demos plus `crates/pdfoxium-viewer/assets/demos/showcase-navigation.pdf`; saved
edits/captures stay under `target/`, and the recorder checks source hashes.

## Publish

GitHub needs a standalone attachment URL in the README for inline playback;
a repository-relative MP4 link opens a file. Keep MP4s and raw takes out of Git.
The encoder writes a poster `.png`, captions `.vtt` and chapter/encoding `.json`
beside the MP4. Copy those three files to [media/](media/) when publishing.

Upload the replacement explicitly from this repository, then replace the
standalone URL in the README with the returned `url`:

```sh
repo_attachment_id=$(gh api 'repos/{owner}/{repo}' --jq .id)
gh api --method POST \
  "https://uploads.github.com/user-attachments/assets?name=viewer-showcase.mp4&content_type=video%2Fmp4&repository_id=$repo_attachment_id" \
  --input target/viewer-reel/viewer-showcase.mp4 \
  -H 'Content-Type: application/octet-stream' \
  -H 'X-GitHub-Api-Version: 2022-11-28'
```

Recording/encoding alone never uploads or changes GitHub content.
