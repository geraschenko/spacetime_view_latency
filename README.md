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

## Update: issue still present with SpacetimeDB v2.2.0

(The above tests were done with v1.11.3)

Also noteworthy is that the default is now "confirmed reads" (wait for roundtrip, IIUC). Even with everything running on the same machine, confirmation adds ~20ms of latency. I bumped the batch size up from 100 to 1000 messages because with batches of 100 and confirmed reads on, the effect was not easy to see. With batches of 1000, it's very clear.

Here are the numbers when subscribing to the view table, with and without confirmed reads:
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to view
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000        93.91        75.53       207.95
      2000       231.90       224.92       483.13
      3000       392.34       384.65       785.04
      4000       546.26       531.76      1103.91
      5000       692.85       677.63      1374.30
      6000       836.93       821.84      1685.42
      7000      1014.01      1015.26      2012.19
      8000      1151.23      1149.68      2300.73
      9000      1297.37      1283.23      2598.40
     10000      1448.20      1431.99      2894.06

$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to view --no-confirmed-reads
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000        71.12        60.09       179.03
      2000       219.05       208.20       471.91
      3000       369.18       356.53       770.28
      4000       513.83       502.01      1058.61
      5000       667.33       656.03      1357.19
      6000       813.82       802.20      1654.01
      7000       968.09       959.19      1962.16
      8000      1109.89      1094.05      2244.93
      9000      1268.35      1254.78      2555.51
     10000      1424.92      1411.83      2866.10
```

Here are the numbers when subscribing to the table directly, with and without confirmed reads:
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to table
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000        54.21        47.46        65.89
      2000        27.83        32.58        33.51
      3000        43.96        45.30        55.17
      4000        41.77        43.23        52.97
      5000        45.07        43.45        53.06
      6000        41.80        42.56        52.85
      7000        41.63        43.35        51.83
      8000        35.70        32.75        41.71
      9000        44.56        49.51        67.56
     10000        40.60        41.82        51.56

$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --server http://127.0.0.1:4000 --clear-database --yes &&
  ./target/release/roundtrip_latency_test --server http://127.0.0.1:4000 --subscribe-to table --no-confirmed-reads
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000        11.81        12.70        19.59
      2000        15.49        14.87        27.25
      3000        13.79        13.48        25.05
      4000        11.88        12.69        22.34
      5000        15.59        16.39        25.25
      6000        14.92        16.21        23.55
      7000        19.51        20.50        30.27
      8000        17.88        18.98        25.93
      9000        14.59        15.86        21.46
     10000        15.83        15.93        26.55
```

## Update: issue still present with SpacetimeDB v2.6.1

Retested on official SpacetimeDB v2.6.1 with the same module and client (the original concurrent path, batches of 1000, confirmed reads on). The behavior is still here: **view-subscription round-trip latency grows roughly linearly with row count, while table-subscription latency stays roughly flat.**

Across 9 complete view runs and 3 complete table runs (10 doses each, 1,000 → 10,000 rows), the view arm averages ~164 ms at 1,000 rows and ~4,027 ms at 10,000 rows (~427 ms per 1,000 rows, linear-fit R² > 0.999 in all 9 runs), while the table arm stays at ~10–11 ms throughout — roughly 16× at 1,000 rows and 350× at 10,000 rows.

The blocks below are the median-slope run in each arm. The full 120-point dataset is in [`docs/reports/data/view-latency-2.6.1.csv`](docs/reports/data/view-latency-2.6.1.csv) and the analysis, methods, and limitations are in [`docs/reports/view-latency-2.6.1-retest.md`](docs/reports/view-latency-2.6.1-retest.md).

View subscription — run `view_r3_a3` (grows with row count):
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \
  ./target/release/roundtrip_latency_test --subscribe-to view
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000       163.20       127.82       458.33
      2000       612.37       577.79      1349.10
      3000      1043.35      1009.40      2198.83
      4000      1469.01      1435.60      3038.12
      5000      1891.45      1858.49      3877.35
      6000      2321.13      2281.45      4735.02
      7000      2741.87      2707.43      5561.73
      8000      3157.72      3127.34      6387.80
      9000      3589.59      3557.09      7245.89
     10000      4038.45      4001.43      8138.07
```

Table subscription — run `table_r1` (stays flat):
```
$ spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \
  ./target/release/roundtrip_latency_test --subscribe-to table
=== SUMMARY ===
 Total messages     Avg (ms)     P50 (ms)     P99 (ms)
      1000        10.33        10.23        14.70
      2000        10.83        11.29        15.15
      3000        10.69        10.36        14.95
      4000        11.13        10.49        15.81
      5000        11.25        11.54        15.68
      6000        11.60        11.40        15.68
      7000        10.89        10.71        15.00
      8000        11.00        10.79        15.57
      9000        10.57        10.83        15.30
     10000        11.58        10.90        15.93
```

![view vs table round-trip latency on SpacetimeDB v2.6.1](docs/reports/view-latency-2.6.1.svg)

Note: absolute latencies in the different version sections above were measured on different hardware and software environments and are not a controlled cross-version benchmark. The signal is the view-vs-table shape difference within a version, not the absolute numbers.
