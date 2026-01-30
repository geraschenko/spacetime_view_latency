# SpacetimeDB view latency scaling issue

This is a minimal reproduction demonstrating that view subscriptions have O(N) latency scaling while table subscriptions remain constant.

The test sends 10 batches of 100 rows to a table and measures the roundtrip time between when the append reducer is called and when it appears in our subscription. If we subscribe to the table directly, the latency remains constant as the size of the table grows. However, if we subscribe to a trivial "pass-through" view of the same table, the latency grows roughly linearly. A possible reason for this is that the cost of rebuilding the backing table for the view grows linearly with the size of the table.

## Setup

This script builds the wasm, generates client bindings, and builds the latency test binary:
```bash
./run_setup.sh
```

## Running the Tests

Test with table subscription (constant latency):
```bash
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \
  ./target/release/roundtrip_latency_test --subscribe-to table

=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100         4.12         4.46         5.29
       200         5.43         5.46         7.47
       300         4.46         4.57         5.75
       400         6.69         6.98         8.89
       500         4.32         4.45         5.53
       600         4.93         5.11         6.38
       700         4.54         4.58         5.66
       800         5.77         5.56         7.86
       900         4.62         4.65         5.74
      1000         4.17         4.21         5.57
```

Test with view subscription (shows linear latency growth):
```bash
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \
  ./target/release/roundtrip_latency_test --subscribe-to view

=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100         6.43         5.98        12.92
       200        12.54        13.61        20.46
       300        16.35        17.00        25.86
       400        17.70        18.14        28.70
       500        22.16        23.06        37.22
       600        25.55        26.47        42.27
       700        26.34        27.08        45.17
       800        24.86        25.14        45.42
       900        30.99        31.43        53.65
      1000        34.62        35.32        61.21
```
