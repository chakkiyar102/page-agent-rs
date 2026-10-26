#!/bin/sh
# Copy the built wasm package into extension/pkg (run ../build.sh first if ../demo/pkg is missing).
set -e
cd "$(dirname "$0")"
mkdir -p pkg
cp ../demo/pkg/page_agent_rs.js ../demo/pkg/page_agent_rs_bg.wasm pkg/
echo "copied pkg/ - load this folder unpacked at chrome://extensions"
