"""Observe real opens of one suite-owned workspace canary on Linux."""
import ctypes
import os
import struct
import sys
from contextlib import contextmanager


@contextmanager
def watch_reads(path):
    if sys.platform != "linux":
        yield None
        return
    libc = ctypes.CDLL(None, use_errno=True)
    fd = libc.inotify_init1(os.O_CLOEXEC | os.O_NONBLOCK)
    if fd < 0:
        raise OSError(ctypes.get_errno(), "inotify_init1 failed")
    try:
        opened, closed, overflow = 0x20, 0x10, 0x4000
        if libc.inotify_add_watch(fd, os.fsencode(path), opened | closed) < 0:
            raise OSError(ctypes.get_errno(), "inotify_add_watch failed")
        def observations():
            counts = [0, 0]
            while True:
                try:
                    buf = os.read(fd, 65536)
                except BlockingIOError:
                    return counts
                offset = 0
                while offset < len(buf):
                    _, mask, _, length = struct.unpack_from("iIII", buf, offset)
                    assert not mask & overflow, "kernel observer lost events"
                    counts[0] += bool(mask & opened)
                    counts[1] += bool(mask & closed)
                    offset += 16 + length
        yield observations
    finally:
        os.close(fd)
