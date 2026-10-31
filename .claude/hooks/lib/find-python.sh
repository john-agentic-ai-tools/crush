# Source this file, then call find_python: it sets PY to a Python >= 3.8
# interpreter on PATH and returns 0, or leaves PY empty and returns 1.
#
# `python` is tried first: on Windows, `python3` can be the Microsoft Store
# stub, which is on PATH but only opens the Store. The version check rejects
# that stub and Linux systems where `python` is still Python 2. 3.8 is the
# floor because bench.py uses assignment expressions.
find_python() {
    PY=""
    local candidate
    for candidate in python python3; do
        if command -v "$candidate" >/dev/null 2>&1 \
            && "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 8))' >/dev/null 2>&1; then
            PY="$candidate"
            return 0
        fi
    done
    return 1
}
