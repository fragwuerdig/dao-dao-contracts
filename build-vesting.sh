#! /bin/bash

set -e

rm -Rf artifacts
mkdir artifacts
cargo build --package cw-vesting-dmz --release --target wasm32-unknown-unknown

mv target/wasm32-unknown-unknown/release/*.wasm artifacts/
rm -Rf target
for i in $BLACKLIST; do
    rm -f artifacts/$i.wasm
done

cd artifacts
mkdir -p opt
for i in *.wasm; do
    wasm-opt -Os --strip-debug $i -o opt/$i
done