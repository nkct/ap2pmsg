import sys
import subprocess
from os.path import dirname, isfile

LIB_PATH=dirname(__file__)
SQLITE_PATH=f"{LIB_PATH}/sqlite3"
BUILD_PATH=f"{LIB_PATH}/build"
LIB_NAME="libap2p"

def main():
    profile = "-g"
    compiler = "gcc"
    lib_ext = "a"
    
    release_mode = False
    if "--release" in sys.argv:
        release_mode = True
        profile = "-Os"
    
    windows_mode = False
    if "--target=x86_64-pc-windows-gnu" in sys.argv:
        windows_mode = True
        compiler = "x86_64-w64-mingw32-gcc"
        lib_ext = "lib"
    
    subprocess.run(["mkdir", "-p", BUILD_PATH])
    
    if not isfile(f"{BUILD_PATH}/libsqlite3.{lib_ext}"):
       print(f"\x1b[32mBuilding\x1b[0m {SQLITE_PATH}/sqlite3.c {'for windows' if windows_mode else ''}")
       subprocess.run([compiler, "-c", "-Os", f"{SQLITE_PATH}/sqlite3.c", "-o", f"{BUILD_PATH}/libsqlite3.{lib_ext}"])
   
    print(f"\x1b[32mBuilding\x1b[0m {LIB_PATH}/{LIB_NAME}.c{' in release mode' if release_mode else ''} {'for windows' if windows_mode else ''}")
    subprocess.run([compiler, profile, "-Wall", "-Wextra", "-c", f"{LIB_PATH}/{LIB_NAME}.c", "-o", f"{BUILD_PATH}/{LIB_NAME}.{lib_ext}"])

if __name__ == "__main__":
    main()