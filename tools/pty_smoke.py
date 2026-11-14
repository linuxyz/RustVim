#!/usr/bin/env python3
"""PTY smoke test for the TUI backend (Unix)."""
import os, pty, select, subprocess, sys, time, fcntl, termios, struct

binary = os.path.abspath("target/debug/rvim")
if not os.path.exists(binary):
    print("build target/debug/rvim first", file=sys.stderr)
    sys.exit(2)

path = "/tmp/rvim-pty.txt"
with open(path, "w") as f:
    f.write("start\n")

try:
    master, slave = pty.openpty()
except OSError as error:
    print(f"skipping PTY smoke test: {error}", file=sys.stderr)
    sys.exit(0)
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
env = dict(os.environ, TERM="xterm-256color", VIMRUNTIME=os.environ.get("VIMRUNTIME", "/nonexistent"))
proc = subprocess.Popen([binary, path], stdin=slave, stdout=slave, stderr=slave, env=env, close_fds=True)
os.close(slave)
time.sleep(0.4)
os.write(master, b"ihello\x1b:wq\r")
deadline = time.time() + 8
output = b""
while time.time() < deadline:
    ready, _, _ = select.select([master], [], [], 0.2)
    if ready:
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        if not chunk:
            break
        output += chunk
    if proc.poll() is not None:
        break
try:
    proc.wait(timeout=2)
except subprocess.TimeoutExpired:
    proc.kill()
    print("TUI did not exit", file=sys.stderr)
    sys.exit(1)
text = open(path).read()
if "hello" not in text:
    print("buffer was not written:", repr(text), file=sys.stderr)
    print(output[-500:], file=sys.stderr)
    sys.exit(1)
print("PTY smoke ok:", repr(text), "frames", output.count(b"\x1b["))
