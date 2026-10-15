-- Backfill a new evm-balances-erc20 database (`extended`) with the history of
-- the RPC-era evm-balances v0.3.4 database (`rpc`). Rename both databases.
--
-- Preconditions:
--   * Same server. The schema embedded in this package is byte-identical to
--     the `blocks` and ERC-20 tables of the one in
--     evm-clickhouse-balances-v0.3.4, except that since v0.7.3 its history keeps
--     only the 1h, 4h, 1d and 1w bars; step 0 checks the tables.
--   * No block is written by both pipelines: the RPC-era sink stopped with its
--     cursor at block S-1 and this package's sink started at block S.
--   * `rpc` stays a separate database. Never copy its `cursors` table, and never
--     create a table without a primary key inside `extended` (the sink refuses
--     to start).
--
-- `rpc` also holds the native tables, which stay out of `extended`.
-- `rpc.blocks` lists the blocks with any balance row (native or ERC-20), so
-- the copied `blocks` history covers both assets' blocks before the cutover
-- block S; from S on, `blocks` lists only blocks with an ERC-20 row.
--
-- Partition copies do not fire materialized views. `INSERT ... SELECT` into
-- erc20_balances would re-run mv_historical_erc20_balances and add the
-- copied rows to the OHLC state a second time (`transactions` double-counts).
-- Copying the state table lets AggregatingMergeTree merge the windows that
-- span block S from both sides. Its 1h, 4h, 1d and 1w bars are copied with
-- `INSERT ... SELECT`, which does not fire the view either (it reads
-- erc20_balances): the RPC-era state also holds 1m to 30m bars, and its table
-- differs in the interval column's default, so it cannot be attached.

-- Steps 0 and 1 throw on failure, so a run of the whole file (for example
-- `clickhouse-client --multiquery < this-file`) stops before step 2 copies
-- anything. A partial copy (blocks attached, a balance table refused) would
-- otherwise need manual cleanup.

-- 0. Check the table structures match: `blocks` and `erc20_balances` exactly, and
--    `historical_erc20_balances_state` by its columns' names and types (the RPC-era
--    table differs only in the interval column's default and comment, and its
--    views in the intervals). The listings show any difference; the assertion
--    stops the run on one. A database set up from another schema, such as the
--    unmerged v0.3.5 variant, cannot be copied. Only this package's tables are
--    compared: `rpc` also holds the native tables, and each sink creates its
--    own `cursors`.
SELECT name, r.q = '' AS missing_in_rpc, e.q = '' AS missing_in_extended
FROM (SELECT name, replaceAll(create_table_query, 'rpc.', '') AS q FROM system.tables WHERE database = 'rpc') AS r
FULL OUTER JOIN (SELECT name, replaceAll(create_table_query, 'extended.', '') AS q FROM system.tables WHERE database = 'extended') AS e
USING (name)
WHERE r.q != e.q
  AND name IN ('blocks', 'erc20_balances');

SELECT
    (SELECT arraySort(groupArray((position, name, type))) FROM system.columns
     WHERE database = 'rpc' AND table = 'historical_erc20_balances_state') AS rpc_columns,
    (SELECT arraySort(groupArray((position, name, type))) FROM system.columns
     WHERE database = 'extended' AND table = 'historical_erc20_balances_state') AS extended_columns
WHERE rpc_columns != extended_columns;

SELECT throwIf(
    (SELECT count() FROM (SELECT name, replaceAll(create_table_query, 'rpc.', '') AS q FROM system.tables WHERE database = 'rpc') AS r
     FULL OUTER JOIN (SELECT name, replaceAll(create_table_query, 'extended.', '') AS q FROM system.tables WHERE database = 'extended') AS e
     USING (name)
     WHERE r.q != e.q AND name IN ('blocks', 'erc20_balances')) > 0
    OR (SELECT arraySort(groupArray((position, name, type))) FROM system.columns
        WHERE database = 'rpc' AND table = 'historical_erc20_balances_state')
       != (SELECT arraySort(groupArray((position, name, type))) FROM system.columns
           WHERE database = 'extended' AND table = 'historical_erc20_balances_state'),
    'rpc and extended table structures differ (listed above): nothing was copied');

-- 1. Check the boundary before copying: rpc_last < extended_first, and
--    extended already has blocks.
SELECT
    (SELECT max(block_num) FROM rpc.blocks)      AS rpc_last,
    (SELECT min(block_num) FROM extended.blocks) AS extended_first;

SELECT throwIf(
    (SELECT count() FROM extended.blocks) = 0
        OR (SELECT max(block_num) FROM rpc.blocks) >= (SELECT min(block_num) FROM extended.blocks),
    'rpc must end before extended starts, and extended must have blocks: nothing was copied');

-- 2. Copy history.
ALTER TABLE extended.blocks                            ATTACH PARTITION tuple() FROM rpc.blocks;
ALTER TABLE extended.erc20_balances                    ATTACH PARTITION tuple() FROM rpc.erc20_balances;
INSERT INTO extended.historical_erc20_balances_state
SELECT * FROM rpc.historical_erc20_balances_state WHERE interval_min IN (60, 240, 1440, 10080);
