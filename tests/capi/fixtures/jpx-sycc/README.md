# Synthetic subsampled JPEG2000 fixtures

These small lossless JP2 images exercise SYCC 4:2:2 and 4:2:0 with even/odd
dimensions, 8/12/16-bit samples, and optional alpha. They contain synthetic
constant samples or a simple coordinate pattern, with no third-party artwork.
Their complete decoded values are asserted through the public PDF API.

`generate.cpp` records how to regenerate them using OpenJPEG's encoder. The
encoder is an optional fixture-generation tool; neither production nor the
test runner links OpenJPEG. Normal tests only read the committed JP2 bytes.

Constant fixtures have Y=100 and neutral chroma at 8-bit scale, giving gray100.
Higher-precision fixtures shift those exact samples by 4 or 8 bits. Alpha
fixtures use alpha128 and are composited over white. Pattern fixtures have
Y=40+20*x+10*y, Cb=112+8*chroma_x and Cr=128+8*chroma_y.
