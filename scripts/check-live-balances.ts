#!/usr/bin/env bun
/**
 * Live check of a balances package against the chain: streams the package's
 * map over recent blocks with `substreams run` and compares every emitted
 * balance with the chain's own value at the same block, `balanceOf` for ERC-20
 * rows (`map_events`, `evm.balances.v1.Events`) and `eth_getBalance` for
 * native rows (`map_sink_entries` since native v0.8.0, hosted-store
 * `SinkEntries` of `evm.balances.v1.NativeBalance`; `map_events`,
 * `evm.balances.v1.NativeEvents`, before). Every row is the account's balance
 * after the block, so it must equal the state at that block number.
 *
 * Usage:
 *   bun scripts/check-live-balances.ts --chain eth --package erc20/balances/substreams.yaml
 *   bun scripts/check-live-balances.ts --chain bsc --package spkg/evm-balances-native-v0.7.5.spkg --blocks 300
 *
 * Options:
 *   --chain <id>        Pinax network id: eth, bsc, polygon, arbone, base, optimism, unichain, hyperevm (required)
 *   --package <path>    manifest or spkg (required)
 *   --blocks <n>        blocks to stream (default 100)
 *   --behind <n>        blocks the range ends behind the head, against reorgs (default 64)
 *   --start <block>     absolute first block instead of head - behind - blocks
 *   --module <name>     map to stream (default map_sink_entries if the package has it, else map_events)
 *   --batch <n>         RPC calls per JSON-RPC batch (default 100)
 *   --parallel <n>      batches in flight (default 4)
 *   --out <file>        write the full report as JSON
 *
 * Environment (never printed):
 *   SUBSTREAMS_API_KEY  key for the Substreams endpoint and, by default, the Pinax RPC
 *   RPC_URL             JSON-RPC URL; default https://<chain>.rpc.pinax.network/v1/$SUBSTREAMS_API_KEY/
 *   SUBSTREAMS_ENDPOINT default <chain>.substreams.pinax.network:443
 *
 * Exits 1 when a balance differs from the chain.
 */

type Row = { block: number; contract?: string; address: string; amount: bigint };
type Checked = Row & { chain?: bigint; error?: string };

const args = new Map<string, string>();
for (let i = 2; i < Bun.argv.length; i += 2) {
    const key = Bun.argv[i];
    if (!key.startsWith('--') || i + 1 >= Bun.argv.length) {
        console.error(`bad argument ${key}; see the usage at the top of ${import.meta.path}`);
        process.exit(2);
    }
    args.set(key.slice(2), Bun.argv[i + 1]);
}
const chain = args.get('chain');
const pkg = args.get('package');
if (!chain || !pkg) {
    console.error('--chain and --package are required');
    process.exit(2);
}
const blocks = Number(args.get('blocks') ?? 100);
const behind = Number(args.get('behind') ?? 64);
const batchSize = Number(args.get('batch') ?? 100);
const parallel = Number(args.get('parallel') ?? 4);
const key = process.env.SUBSTREAMS_API_KEY ?? '';
const rpcUrl = process.env.RPC_URL ?? `https://${chain}.rpc.pinax.network/v1/${key}/`;
const endpoint = process.env.SUBSTREAMS_ENDPOINT ?? `${chain}.substreams.pinax.network:443`;

/** Removes the API key and the RPC URL from text before it is printed. */
function redact(text: string): string {
    let out = text.split(rpcUrl).join('<rpc>');
    if (key) out = out.split(key).join('***');
    return out;
}

async function rpc(calls: { method: string; params: unknown[] }[]): Promise<{ result?: string; error?: string }[]> {
    const body = calls.map((c, id) => ({ jsonrpc: '2.0', id, ...c }));
    for (let attempt = 1; ; attempt++) {
        try {
            const response = await fetch(rpcUrl, {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            type Reply = { id: number; result?: string; error?: { message?: string } };
            // Some nodes answer a batch of one call with a bare object.
            const json = (await response.json()) as Reply | Reply[];
            const replies = Array.isArray(json) ? json : [json];
            const byId = new Map(replies.map((r) => [r.id, r]));
            return body.map((b) => {
                const r = byId.get(b.id);
                if (!r) return { error: 'no reply' };
                if (r.error) return { error: redact(r.error.message ?? JSON.stringify(r.error)) };
                return { result: r.result };
            });
        } catch (error) {
            if (attempt >= 3) return calls.map(() => ({ error: redact(String(error)) }));
            await Bun.sleep(1000 * attempt);
        }
    }
}

async function headBlock(): Promise<number> {
    const [reply] = await rpc([{ method: 'eth_blockNumber', params: [] }]);
    if (!reply.result) throw new Error(`eth_blockNumber failed: ${reply.error}`);
    return Number.parseInt(reply.result, 16);
}

/** `map_sink_entries` when the package has it (native v0.8.0 and later), else `map_events`. */
async function moduleName(): Promise<string> {
    const proc = Bun.spawn(['substreams', 'info', pkg as string], { stdout: 'pipe', stderr: 'pipe' });
    const info = await new Response(proc.stdout).text();
    if ((await proc.exited) !== 0) throw new Error(`substreams info exited with ${proc.exitCode}`);
    return /^Name: map_sink_entries$/m.test(info) ? 'map_sink_entries' : 'map_events';
}

/** `0x` + lowercase hex of a JSON bytes field, which the CLI renders as hex or base64. */
function hexBytes(value: string): string {
    if (value.startsWith('0x')) return value.toLowerCase();
    return `0x${Buffer.from(value, 'base64').toString('hex')}`;
}

async function stream(module: string, start: number): Promise<{ rows: Row[]; type: string }> {
    const cmd = ['substreams', 'run', pkg as string, module, '-e', endpoint, '-s', String(start), '-t', `+${blocks}`, '-o', 'jsonl'];
    const proc = Bun.spawn(cmd, { stdout: 'pipe', stderr: 'pipe' });
    const rows: Row[] = [];
    let type = '';
    let buffer = '';
    const decoder = new TextDecoder();
    for await (const chunk of proc.stdout) {
        buffer += decoder.decode(chunk, { stream: true });
        let newline = buffer.indexOf('\n');
        while (newline >= 0) {
            const line = buffer.slice(0, newline).trim();
            buffer = buffer.slice(newline + 1);
            newline = buffer.indexOf('\n');
            if (!line) continue;
            const message = JSON.parse(line);
            type ||= message['@type'];
            for (const b of message['@data']?.balances ?? []) {
                rows.push({ block: message['@block'], contract: b.contract, address: b.address, amount: BigInt(b.amount) });
            }
            for (const e of message['@data']?.entries ?? []) {
                const address = hexBytes(e.value.address);
                if (address !== hexBytes(e.key.bytes)) throw new Error(`entry key ${e.key.bytes} is not its balance's address ${address}`);
                rows.push({ block: message['@block'], address, amount: BigInt(e.value.amount) });
            }
        }
    }
    const code = await proc.exited;
    if (code !== 0) {
        const stderr = await new Response(proc.stderr).text();
        throw new Error(`substreams run exited with ${code}: ${redact(stderr.slice(-2000))}`);
    }
    return { rows, type };
}

/** The chain's value for a row at its block: `balanceOf` or `eth_getBalance`. */
function call(row: Row): { method: string; params: unknown[] } {
    const block = `0x${row.block.toString(16)}`;
    if (row.contract === undefined) return { method: 'eth_getBalance', params: [row.address, block] };
    const data = `0x70a08231${row.address.slice(2).toLowerCase().padStart(64, '0')}`;
    return { method: 'eth_call', params: [{ to: row.contract, data }, block] };
}

/** Queries the chain's value of each row, `size` calls per batch, `workers` batches in flight. */
async function query(rows: Row[], size: number, workers: number): Promise<Checked[]> {
    const checked: Checked[] = [];
    const batches: Row[][] = [];
    for (let i = 0; i < rows.length; i += size) batches.push(rows.slice(i, i + size));
    let next = 0;
    async function worker() {
        while (next < batches.length) {
            const batch = batches[next++];
            const replies = await rpc(batch.map(call));
            batch.forEach((row, i) => {
                const reply = replies[i];
                // balanceOf returns a uint256 in its first 32-byte word; as in ABI
                // decoding, any further bytes are ignored. A shorter answer is not
                // an ERC-20 balance.
                const value = row.contract !== undefined && reply.result !== undefined ? reply.result.slice(0, 66) : reply.result;
                if (value === undefined || (row.contract !== undefined && value.length !== 66)) {
                    checked.push({ ...row, error: reply.error ?? `unexpected result ${reply.result}` });
                } else {
                    checked.push({ ...row, chain: BigInt(value) });
                }
            });
        }
    }
    await Promise.all(Array.from({ length: workers }, worker));
    return checked;
}

/** Rate-limit errors (HTTP 429, "request limit") are retried in up to three slower passes. */
const limited = (row: Checked) => row.error !== undefined && /429|limit/i.test(row.error);

async function check(rows: Row[]): Promise<Checked[]> {
    let checked = await query(rows, batchSize, parallel);
    for (let pass = 1; pass <= 3 && checked.some(limited); pass++) {
        await Bun.sleep(2000 * pass);
        const retried = await query(checked.filter(limited), 10, 1);
        checked = checked.filter((row) => !limited(row)).concat(retried);
    }
    return checked;
}

const head = await headBlock();
const start = args.has('start') ? Number(args.get('start')) : head - behind - blocks;
const mapModule = args.get('module') ?? (await moduleName());
console.log(`${chain}: streaming ${pkg} ${mapModule} over [${start}, ${start + blocks}) (head ${head})`);
const { rows, type } = await stream(mapModule, start);
console.log(`${rows.length} balances (${type}); checking against the chain`);
const checked = await check(rows);

const byContract = new Map<string, { checked: number; matched: number; differ: Checked[]; errors: Checked[] }>();
for (const row of checked) {
    const id = row.contract ?? 'native';
    const entry = byContract.get(id) ?? { checked: 0, matched: 0, differ: [], errors: [] };
    entry.checked++;
    if (row.error !== undefined) entry.errors.push(row);
    else if (row.chain === row.amount) entry.matched++;
    else entry.differ.push(row);
    byContract.set(id, entry);
}
const total = { checked: 0, matched: 0, differ: 0, errors: 0 };
for (const e of byContract.values()) {
    total.checked += e.checked;
    total.matched += e.matched;
    total.differ += e.differ.length;
    total.errors += e.errors.length;
}
const scope = type.endsWith('NativeEvents') || type.endsWith('SinkEntries') ? 'native balances' : `balances of ${byContract.size} contracts`;
console.log(`checked ${total.checked} ${scope}: ${total.matched} match, ${total.differ} differ, ${total.errors} RPC errors`);
const worst = [...byContract.entries()].filter(([, e]) => e.differ.length > 0).sort((a, b) => b[1].differ.length - a[1].differ.length);
for (const [contract, e] of worst.slice(0, 30)) {
    const d = e.differ[0];
    console.log(
        `  ${contract}: ${e.differ.length}/${e.checked} differ, e.g. ${d.address} at ${d.block}: package ${d.amount}, chain ${d.chain} (diff ${(d.chain as bigint) - d.amount})`,
    );
}
const errored = [...byContract.entries()].filter(([, e]) => e.errors.length > 0);
for (const [contract, e] of errored.slice(0, 10)) console.log(`  ${contract}: ${e.errors.length} RPC errors, e.g. ${e.errors[0].error}`);

const out = args.get('out');
if (out) {
    const report = {
        chain,
        package: pkg,
        type,
        range: [start, start + blocks],
        head,
        total,
        contracts: Object.fromEntries(
            [...byContract.entries()].map(([c, e]) => [
                c,
                {
                    checked: e.checked,
                    matched: e.matched,
                    differ: e.differ.map((r) => ({ address: r.address, block: r.block, package: String(r.amount), chain: String(r.chain) })),
                    errors: e.errors.map((r) => ({ address: r.address, block: r.block, package: String(r.amount), error: r.error })),
                },
            ]),
        ),
    };
    await Bun.write(out, JSON.stringify(report, null, 1));
    console.log(`report written to ${out}`);
}
process.exit(total.differ > 0 ? 1 : 0);
