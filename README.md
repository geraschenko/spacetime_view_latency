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

## Update: issue still present with SpacetimeDB v2.0.5

(The above tests were done with v1.11.3)

Also noteworthy is that the default is now "confirmed reads" (wait for roundtrip, IIUC). Even with everything running on the same machine, confirmation adds ~30ms of latency.

Here are the numbers when subscribing to the view table, with and without confirmed reads:
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to view
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100        34.48        34.83        34.97
       200        43.33        49.28        49.40
       300        43.83        46.46        56.03
       400        41.65        41.62        51.10
       500        43.89        43.84        53.22
       600        48.58        47.00        57.03
       700        42.59        39.94        57.20
       800        49.97        47.88        68.40
       900        52.64        55.58        65.80
      1000        56.04        56.70        78.16

$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to view --no-confirmed-reads
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100         5.90         5.96         8.95
       200        14.72        16.77        22.46
       300        20.79        23.40        32.30
       400        18.46        16.32        30.45
       500        21.74        22.97        34.56
       600        25.03        26.38        41.86
       700        25.70        26.11        43.53
       800        25.81        27.11        45.99
       900        25.41        25.87        49.26
      1000        32.91        34.73        55.83
```

Here are the numbers when subscribing to the table directly, with and without confirmed reads:
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to table
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100        33.92        34.28        34.42
       200        34.11        34.46        34.57
       300        33.30        33.64        33.76
       400        31.83        32.14        32.30
       500        38.15        38.57        38.72
       600        33.83        34.21        34.34
       700        33.74        34.11        34.23
       800        34.26        34.60        34.64
       900        40.61        41.01        41.30
      1000        56.90        57.65        57.70

$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to table --no-confirmed-reads
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
       100         4.68         4.75         5.82
       200         7.25         7.81         9.67
       300         6.91         7.27         8.81
       400         5.87         6.20         7.77
       500         3.75         3.85         5.31
       600         5.38         5.71         7.03
       700         5.30         5.70         7.01
       800         7.09         7.81         9.52
       900         8.73         9.29        11.73
      1000         3.84         4.06         5.55
```
