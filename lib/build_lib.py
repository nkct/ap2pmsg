import sys
import subprocess
from os.path import dirname, isfile

LIB_PATH=dirname(__file__)
SQLITE_PATH=f"{LIB_PATH}/sqlite3"
BUILD_PATH=f"{LIB_PATH}/build"
LIB_NAME="libap2p"

def main():
    release_mode = False
    if "--release" in sys.argv:
        release_mode = True
    profile = "-Os" if release_mode else "-g"
    
    subprocess.run(["mkdir", "-p", BUILD_PATH])
    
    if not isfile(f"{BUILD_PATH}/libsqlite3.a"):
       print(f"\x1b[32mBuilding\x1b[0m {SQLITE_PATH}/sqlite3.c")
       subprocess.run(["gcc", "-c", "-Os", f"{SQLITE_PATH}/sqlite3.c", "-o", f"{BUILD_PATH}/libsqlite3.a"])
   
    print(f"\x1b[32mBuilding\x1b[0m {LIB_PATH}/{LIB_NAME}.c{' in release mode' if release_mode else ''}")
    subprocess.run(["gcc", profile, "-c", f"{LIB_PATH}/{LIB_NAME}.c", "-o", f"{BUILD_PATH}/{LIB_NAME}.a"])

if __name__ == "__main__":
    main()