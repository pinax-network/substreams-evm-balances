.DEFAULT_GOAL := pack

.PHONY: pack
pack:
	buf build . --as-file-descriptor-set -o sink.binpb
	substreams pack substreams.yaml -o ../../spkg/evm-balances-sink-protodefs-v0.2.0.spkg
