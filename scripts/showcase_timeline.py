# Copyright 2026 The PDFium Authors
# Use of this source code is governed by a BSD-style license that can be
# found in upstream/pdfium/LICENSE.
"""Apply explicit setup cuts without changing the speed within a scene."""
from copy import deepcopy


def playback_timeline(source):
    data = deepcopy(source)
    cuts = []
    start = None
    for event in data['events']:
        if event['kind'] == 'cut_start':
            assert start is None
            start = event['t']
        elif event['kind'] == 'cut_end':
            assert start is not None and event['t'] >= start
            cuts.append((start, event['t']))
            start = None
    assert start is None

    def time(t):
        return t - sum(max(0, min(t, end) - start) for start, end in cuts)

    def kept(t):
        return not any(start <= t < end for start, end in cuts)

    frames = [frame for frame in data['frames'] if kept(frame['t'])]
    for start, end in cuts:
        # The last prepared app frame is already available at a hard cut.
        prepared = [frame for frame in data['frames'] if start <= frame['t'] < end]
        if prepared:
            frame = prepared[-1].copy()
            frame['t'] = end
            frames.append(frame)
    frames.sort(key=lambda frame: frame['t'])
    data['frames'] = frames
    data['events'] = [event for event in data['events']
                      if not event['kind'].startswith('cut_') and kept(event['t'])]
    for items in (data['frames'], data['events'], data['calls']):
        for item in items:
            item['t'] = time(item['t'])
    data['raw_duration'] = data['duration']
    data['duration'] = time(data['duration'])
    data['setup_cut_seconds'] = sum(end - start for start, end in cuts)
    return data
