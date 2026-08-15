# Azums M14 Performance Report

- generated_at_unix_ms: 1786783914979
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 177177.39 | 2.4189 | 2.6789 | 2.6789 | 2.6789 |
| memory | small_jobs | 2 | 175502.05 | 2.9811 | 3.4361 | 3.4361 | 3.4361 |
| memory | small_jobs | 4 | 170020.65 | 2.9206 | 3.1805 | 3.1805 | 3.1805 |
| memory | small_jobs | 8 | 162883.20 | 2.8624 | 2.9499 | 2.9499 | 2.9499 |
| memory | small_jobs | 16 | 162937.65 | 2.8754 | 2.9035 | 2.9035 | 2.9035 |
| memory | small_jobs | 32 | 163072.08 | 2.8568 | 2.8827 | 2.8827 | 2.8827 |
| memory | large_payloads | 1 | 81590.20 | 3.5082 | 4.5267 | 4.5267 | 4.5267 |
| memory | large_payloads | 2 | 106357.62 | 4.5846 | 5.3420 | 5.3420 | 5.3420 |
| memory | large_payloads | 4 | 105837.55 | 4.2730 | 4.6263 | 4.6263 | 4.6263 |
| memory | large_payloads | 8 | 124393.65 | 4.0795 | 4.0925 | 4.0925 | 4.0925 |
| memory | large_payloads | 16 | 124271.17 | 4.0872 | 4.1841 | 4.1841 | 4.1841 |
| memory | large_payloads | 32 | 120956.12 | 4.1113 | 4.2015 | 4.2015 | 4.2015 |
| memory | batch_jobs | 1 | 197258.14 | 2.2485 | 2.2870 | 2.2870 | 2.2870 |
| memory | batch_jobs | 2 | 193351.16 | 2.4550 | 2.4831 | 2.4831 | 2.4831 |
| memory | batch_jobs | 4 | 177583.05 | 2.8615 | 2.9664 | 2.9664 | 2.9664 |
| memory | batch_jobs | 8 | 180797.14 | 2.8044 | 2.8368 | 2.8368 | 2.8368 |
| memory | batch_jobs | 16 | 185541.33 | 2.8210 | 2.8544 | 2.8544 | 2.8544 |
| memory | batch_jobs | 32 | 180767.20 | 2.8422 | 2.8422 | 2.8422 | 2.8422 |
| memory | mixed_priorities | 1 | 206497.97 | 2.2076 | 2.2441 | 2.2441 | 2.2441 |
| memory | mixed_priorities | 2 | 194644.73 | 2.4406 | 2.5022 | 2.5022 | 2.5022 |
| memory | mixed_priorities | 4 | 176597.41 | 2.8923 | 3.0134 | 3.0134 | 3.0134 |
| memory | mixed_priorities | 8 | 182771.83 | 2.8464 | 2.8486 | 2.8486 | 2.8486 |
| memory | mixed_priorities | 16 | 183368.73 | 2.8046 | 2.8312 | 2.8312 | 2.8312 |
| memory | mixed_priorities | 32 | 186743.49 | 2.8500 | 2.8663 | 2.8663 | 2.8663 |
| memory | high_contention | 1 | 128433.46 | 2.2905 | 2.3679 | 2.3679 | 2.3679 |
| memory | high_contention | 2 | 122974.31 | 2.5646 | 2.7184 | 2.7184 | 2.7184 |
| memory | high_contention | 4 | 118528.20 | 2.9061 | 2.9614 | 2.9614 | 2.9614 |
| memory | high_contention | 8 | 118663.40 | 2.9223 | 2.9430 | 2.9430 | 2.9430 |
| memory | high_contention | 16 | 116902.84 | 2.9280 | 3.0013 | 3.0013 | 3.0013 |
| memory | high_contention | 32 | 117849.22 | 2.9626 | 2.9724 | 2.9724 | 2.9724 |
| memory | idle_queue | 1 | 0.00 | 0.0004 | 0.0011 | 0.0011 | 0.0011 |
| memory | idle_queue | 2 | 0.00 | 0.0006 | 0.0006 | 0.0006 | 0.0006 |
| memory | idle_queue | 4 | 0.00 | 0.0008 | 0.0015 | 0.0015 | 0.0015 |
| memory | idle_queue | 8 | 0.00 | 0.0015 | 0.0025 | 0.0025 | 0.0025 |
| memory | idle_queue | 16 | 0.00 | 0.0029 | 0.0047 | 0.0047 | 0.0047 |
| memory | idle_queue | 32 | 0.00 | 0.0054 | 0.0068 | 0.0068 | 0.0068 |
| sqlite | small_jobs | 1 | 2156.84 | 377.8441 | 382.1947 | 382.1947 | 382.1947 |
| sqlite | large_payloads | 1 | 1811.77 | 433.6177 | 436.5258 | 436.5258 | 436.5258 |
| sqlite | batch_jobs | 1 | 2146.68 | 383.8361 | 384.8006 | 384.8006 | 384.8006 |
| sqlite | mixed_priorities | 1 | 2149.91 | 381.4044 | 384.9544 | 384.9544 | 384.9544 |
| sqlite | high_contention | 1 | 2185.99 | 373.7356 | 376.5605 | 376.5605 | 376.5605 |
| sqlite | idle_queue | 1 | 0.00 | 0.2400 | 0.2401 | 0.2401 | 0.2401 |
| sqlite | idle_queue | 2 | 0.00 | 0.4521 | 0.4522 | 0.4522 | 0.4522 |
| sqlite | idle_queue | 4 | 0.00 | 0.7241 | 0.7485 | 0.7485 | 0.7485 |
| sqlite | idle_queue | 8 | 0.00 | 1.2721 | 1.2830 | 1.2830 | 1.2830 |
| sqlite | idle_queue | 16 | 0.00 | 2.3549 | 2.3588 | 2.3588 | 2.3588 |
| sqlite | idle_queue | 32 | 0.00 | 4.4572 | 4.4746 | 4.4746 | 4.4746 |
| postgres | small_jobs | 1 | 300.87 | 1509.7326 | 1683.6248 | 1683.6248 | 1683.6248 |
| postgres | small_jobs | 2 | 335.28 | 1144.6871 | 1229.3050 | 1229.3050 | 1229.3050 |
| postgres | small_jobs | 4 | 363.01 | 925.9092 | 928.1427 | 928.1427 | 928.1427 |
| postgres | small_jobs | 8 | 341.07 | 1046.9694 | 1139.7026 | 1139.7026 | 1139.7026 |
| postgres | small_jobs | 16 | 323.07 | 1215.7010 | 1254.0400 | 1254.0400 | 1254.0400 |
| postgres | small_jobs | 32 | 299.34 | 1358.3704 | 1394.5349 | 1394.5349 | 1394.5349 |
| postgres | large_payloads | 1 | 178.15 | 3396.7744 | 3416.5324 | 3416.5324 | 3416.5324 |
| postgres | large_payloads | 2 | 236.33 | 1849.0971 | 2344.3171 | 2344.3171 | 2344.3171 |
| postgres | large_payloads | 4 | 268.65 | 1445.7545 | 1485.4703 | 1485.4703 | 1485.4703 |
| postgres | large_payloads | 8 | 258.57 | 1570.4644 | 1616.7737 | 1616.7737 | 1616.7737 |
| postgres | large_payloads | 16 | 248.34 | 1701.4381 | 1739.3568 | 1739.3568 | 1739.3568 |
| postgres | large_payloads | 32 | 239.00 | 1827.1616 | 1877.3398 | 1877.3398 | 1877.3398 |
| postgres | batch_jobs | 1 | 196.36 | 2863.4696 | 3318.1610 | 3318.1610 | 3318.1610 |
| postgres | batch_jobs | 2 | 237.00 | 2089.4721 | 2136.9270 | 2136.9270 | 2136.9270 |
| postgres | batch_jobs | 4 | 263.25 | 1631.1539 | 1674.5867 | 1674.5867 | 1674.5867 |
| postgres | batch_jobs | 8 | 253.39 | 1747.1643 | 1825.5451 | 1825.5451 | 1825.5451 |
| postgres | batch_jobs | 16 | 242.98 | 1893.4624 | 1956.7183 | 1956.7183 | 1956.7183 |
| postgres | batch_jobs | 32 | 250.05 | 1716.6176 | 1790.2348 | 1790.2348 | 1790.2348 |
| postgres | mixed_priorities | 1 | 167.07 | 3743.8576 | 3842.8250 | 3842.8250 | 3842.8250 |
| postgres | mixed_priorities | 2 | 202.83 | 2649.2022 | 2718.5049 | 2718.5049 | 2718.5049 |
| postgres | mixed_priorities | 4 | 234.02 | 2012.8825 | 2029.2495 | 2029.2495 | 2029.2495 |
| postgres | mixed_priorities | 8 | 235.48 | 1843.6795 | 2147.5434 | 2147.5434 | 2147.5434 |
| postgres | mixed_priorities | 16 | 231.00 | 2018.0947 | 2021.2526 | 2021.2526 | 2021.2526 |
| postgres | mixed_priorities | 32 | 224.11 | 2099.5606 | 2112.0530 | 2112.0530 | 2112.0530 |
| postgres | high_contention | 1 | 143.87 | 4435.1883 | 4863.1678 | 4863.1678 | 4863.1678 |
| postgres | high_contention | 2 | 189.79 | 2888.4287 | 2891.6568 | 2891.6568 | 2891.6568 |
| postgres | high_contention | 4 | 221.36 | 2128.6747 | 2133.0524 | 2133.0524 | 2133.0524 |
| postgres | high_contention | 8 | 214.90 | 2194.5902 | 2270.3425 | 2270.3425 | 2270.3425 |
| postgres | high_contention | 16 | 201.72 | 2501.5765 | 2585.9425 | 2585.9425 | 2585.9425 |
| postgres | high_contention | 32 | 193.54 | 2464.8629 | 3139.7031 | 3139.7031 | 3139.7031 |
| postgres | idle_queue | 1 | 0.00 | 205.6598 | 205.7430 | 205.7430 | 205.7430 |
| postgres | idle_queue | 2 | 0.00 | 412.1307 | 419.8500 | 419.8500 | 419.8500 |
| postgres | idle_queue | 4 | 0.00 | 492.1842 | 500.0638 | 500.0638 | 500.0638 |
| postgres | idle_queue | 8 | 0.00 | 644.4437 | 644.7641 | 644.7641 | 644.7641 |
| postgres | idle_queue | 16 | 0.00 | 814.5528 | 818.2621 | 818.2621 | 818.2621 |
| postgres | idle_queue | 32 | 0.00 | 910.4553 | 912.1231 | 912.1231 | 912.1231 |
| redis | small_jobs | 1 | 800.69 | 820.9383 | 831.3167 | 831.3167 | 831.3167 |
| redis | small_jobs | 2 | 1013.87 | 565.9831 | 567.5628 | 567.5628 | 567.5628 |
| redis | small_jobs | 4 | 1355.27 | 323.5329 | 323.8802 | 323.8802 | 323.8802 |
| redis | small_jobs | 8 | 1648.30 | 194.4902 | 194.6642 | 194.6642 | 194.6642 |
| redis | small_jobs | 16 | 1849.74 | 118.3735 | 121.6820 | 121.6820 | 121.6820 |
| redis | small_jobs | 32 | 2040.75 | 73.7273 | 74.3154 | 74.3154 | 74.3154 |
| redis | large_payloads | 1 | 698.72 | 954.3182 | 971.8590 | 971.8590 | 971.8590 |
| redis | large_payloads | 2 | 915.66 | 619.2258 | 634.4274 | 634.4274 | 634.4274 |
| redis | large_payloads | 4 | 1140.13 | 407.2115 | 408.7481 | 408.7481 | 408.7481 |
| redis | large_payloads | 8 | 1372.80 | 254.5556 | 260.7400 | 260.7400 | 260.7400 |
| redis | large_payloads | 16 | 1544.63 | 171.3847 | 171.6068 | 171.6068 | 171.6068 |
| redis | large_payloads | 32 | 1678.98 | 126.3998 | 127.9269 | 127.9269 | 127.9269 |
| redis | batch_jobs | 1 | 806.36 | 818.6305 | 827.0809 | 827.0809 | 827.0809 |
| redis | batch_jobs | 2 | 1013.69 | 563.1356 | 567.4684 | 567.4684 | 567.4684 |
| redis | batch_jobs | 4 | 1335.78 | 323.4981 | 338.5989 | 338.5989 | 338.5989 |
| redis | batch_jobs | 8 | 1635.34 | 192.8133 | 192.8210 | 192.8210 | 192.8210 |
| redis | batch_jobs | 16 | 1847.53 | 120.8754 | 125.1795 | 125.1795 | 125.1795 |
| redis | batch_jobs | 32 | 2031.34 | 74.5545 | 74.7386 | 74.7386 | 74.7386 |
| redis | mixed_priorities | 1 | 801.52 | 827.3083 | 840.2274 | 840.2274 | 840.2274 |
| redis | mixed_priorities | 2 | 1004.64 | 577.3940 | 577.4803 | 577.4803 | 577.4803 |
| redis | mixed_priorities | 4 | 1326.34 | 332.3706 | 335.5647 | 335.5647 | 335.5647 |
| redis | mixed_priorities | 8 | 1605.79 | 196.8591 | 203.9533 | 203.9533 | 203.9533 |
| redis | mixed_priorities | 16 | 1871.92 | 117.3085 | 118.5050 | 118.5050 | 118.5050 |
| redis | mixed_priorities | 32 | 2050.23 | 71.0062 | 71.5690 | 71.5690 | 71.5690 |
| redis | high_contention | 1 | 711.72 | 826.3728 | 874.0005 | 874.0005 | 874.0005 |
| redis | high_contention | 2 | 890.03 | 561.8849 | 576.7916 | 576.7916 | 576.7916 |
| redis | high_contention | 4 | 1122.40 | 327.3668 | 333.4654 | 333.4654 | 333.4654 |
| redis | high_contention | 8 | 1323.63 | 196.9255 | 197.9528 | 197.9528 | 197.9528 |
| redis | high_contention | 16 | 1472.14 | 119.2065 | 123.4632 | 123.4632 | 123.4632 |
| redis | high_contention | 32 | 1584.01 | 72.4835 | 72.6162 | 72.6162 | 72.6162 |
| redis | idle_queue | 1 | 0.00 | 0.1339 | 0.1393 | 0.1393 | 0.1393 |
| redis | idle_queue | 2 | 0.00 | 0.2568 | 0.2913 | 0.2913 | 0.2913 |
| redis | idle_queue | 4 | 0.00 | 0.4994 | 0.5062 | 0.5062 | 0.5062 |
| redis | idle_queue | 8 | 0.00 | 0.9514 | 1.0130 | 1.0130 | 1.0130 |
| redis | idle_queue | 16 | 0.00 | 1.9341 | 1.9551 | 1.9551 | 1.9551 |
| redis | idle_queue | 32 | 0.00 | 3.8909 | 3.8971 | 3.8971 | 3.8971 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
