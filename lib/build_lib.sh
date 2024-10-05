LIB_PATH=$(dirname "$0")

echo -e "\x1b[32mBuilding\x1b[0m $LIB_PATH/libap2p.c"
gcc -c -o "$LIB_PATH/libap2p.o" "$LIB_PATH/libap2p.c" -lsqlite3
ar rcs "$LIB_PATH/libap2p.a" "$LIB_PATH/libap2p.o"
rm -f "$LIB_PATH/libap2p.o"