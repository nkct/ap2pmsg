LIB_PATH=$(dirname "$0")

if [ ! -f "$LIB_PATH/sqlite3/sqlite3.a" ]; then
    echo -e "\x1b[32mBuilding\x1b[0m $LIB_PATH/sqlite3/sqlite3.c"
    gcc -c -Os "$LIB_PATH/sqlite3/sqlite3.c" -o "$LIB_PATH/sqlite3/sqlite3.a"
fi

# TODO: control debug and release profiles with env args and pass them in from build.rs
echo -e "\x1b[32mBuilding\x1b[0m $LIB_PATH/libap2p.c"
gcc -g -c "$LIB_PATH/libap2p.c" -o "$LIB_PATH/libap2p.a" -L"$LIB_PATH/sqlite3" -lsqlite3
