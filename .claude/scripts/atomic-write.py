#!/usr/bin/env python3
import sys, json, os, tempfile

def atomic_write(path, data):
    dir_ = os.path.dirname(path) or '.'
    fd, tmp = tempfile.mkstemp(dir=dir_, suffix='.tmp')
    try:
        with os.fdopen(fd, 'w') as f:
            json.dump(data, f, indent=2)
        os.replace(tmp, path)
    except:
        try: os.unlink(tmp)
        except: pass
        raise

if __name__ == '__main__':
    path = sys.argv[1]
    data = json.loads(sys.stdin.read())
    atomic_write(path, data)
