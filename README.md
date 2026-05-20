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
