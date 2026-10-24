# rpc-dashboard module

This reference module demonstrates the JSONL RPC foundation. Copy the
directory to `$XDG_CONFIG_HOME/groov/modules/rpc-dashboard/`, run `:reload`,
and enable it with `:modules on rpc-dashboard`.

The module receives one JSON request on stdin for each event or command and
returns JSON response lines. It demonstrates a status response for playback
events and a paragraph view for the `:dashboard` command or `D` key.
