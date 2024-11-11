LIB_PATH=$(dirname "$0")
SQLITE_PATH="$LIB_PATH/sqlite3"
BUILD_PATH="$LIB_PATH/build"
LIB_NAME="libap2p"

mkdir -p $BUILD_PATH

if [ ! -f "$BUILD_PATH/libsqlite3.a" ]; then
    echo -e "\x1b[32mBuilding\x1b[0m $SQLITE_PATH/sqlite3.c"
    gcc -c -Os "$SQLITE_PATH/sqlite3.c" -o "$BUILD_PATH/libsqlite3.a"
fi

echo -e "\x1b[32mBuilding\x1b[0m $LIB_PATH/$LIB_NAME.c"
if [ $1 == "--release" ]; then
    gcc -c -O2 "$LIB_PATH/$LIB_NAME.c" -o "$BUILD_PATH/$LIB_NAME.a"
else
    gcc -g -c "$LIB_PATH/$LIB_NAME.c" -o "$BUILD_PATH/$LIB_NAME.a"
fi
