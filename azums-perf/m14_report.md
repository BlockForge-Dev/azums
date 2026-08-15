# Azums M14 Performance Report

- generated_at_unix_ms: 1786771285732
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 103856.51 | 6.7588 | 8.0691 | 8.0691 | 8.0691 |
| memory | small_jobs | 2 | 103207.30 | 7.6048 | 8.2303 | 8.2303 | 8.2303 |
| memory | small_jobs | 4 | 99053.25 | 7.7849 | 9.0449 | 9.0449 | 9.0449 |
| memory | small_jobs | 8 | 104718.20 | 7.6005 | 7.7042 | 7.7042 | 7.7042 |
| memory | small_jobs | 16 | 99364.45 | 7.7840 | 7.9718 | 7.9718 | 7.9718 |
| memory | small_jobs | 32 | 94552.96 | 8.2846 | 8.4699 | 8.4699 | 8.4699 |
| memory | large_payloads | 1 | 34742.97 | 21.6660 | 27.2706 | 27.2706 | 27.2706 |
| memory | large_payloads | 2 | 34248.44 | 24.3850 | 29.2384 | 29.2384 | 29.2384 |
| memory | large_payloads | 4 | 27347.04 | 29.9363 | 39.6523 | 39.6523 | 39.6523 |
| memory | large_payloads | 8 | 31813.39 | 28.0463 | 28.4483 | 28.4483 | 28.4483 |
| memory | large_payloads | 16 | 31841.01 | 27.6454 | 29.9795 | 29.9795 | 29.9795 |
| memory | large_payloads | 32 | 30545.53 | 29.7350 | 30.4098 | 30.4098 | 30.4098 |
| memory | batch_jobs | 1 | 113731.10 | 6.7615 | 6.8074 | 6.8074 | 6.8074 |
| memory | batch_jobs | 2 | 107531.92 | 7.2409 | 7.4568 | 7.4568 | 7.4568 |
| memory | batch_jobs | 4 | 102098.64 | 7.7116 | 8.0748 | 8.0748 | 8.0748 |
| memory | batch_jobs | 8 | 103616.43 | 7.6408 | 7.7265 | 7.7265 | 7.7265 |
| memory | batch_jobs | 16 | 101461.61 | 7.8869 | 7.9201 | 7.9201 | 7.9201 |
| memory | batch_jobs | 32 | 96004.13 | 8.3929 | 8.6069 | 8.6069 | 8.6069 |
| memory | mixed_priorities | 1 | 114802.74 | 6.7949 | 6.8107 | 6.8107 | 6.8107 |
| memory | mixed_priorities | 2 | 103421.80 | 7.7360 | 7.7632 | 7.7632 | 7.7632 |
| memory | mixed_priorities | 4 | 102144.43 | 7.8764 | 7.9911 | 7.9911 | 7.9911 |
| memory | mixed_priorities | 8 | 101702.45 | 8.0616 | 8.1424 | 8.1424 | 8.1424 |
| memory | mixed_priorities | 16 | 99929.05 | 8.0002 | 8.0870 | 8.0870 | 8.0870 |
| memory | mixed_priorities | 32 | 97106.73 | 8.3435 | 8.5350 | 8.5350 | 8.5350 |
| memory | high_contention | 1 | 79053.85 | 7.1549 | 7.2651 | 7.2651 | 7.2651 |
| memory | high_contention | 2 | 76168.75 | 7.9499 | 7.9535 | 7.9535 | 7.9535 |
| memory | high_contention | 4 | 77178.35 | 7.7530 | 7.8591 | 7.8591 | 7.8591 |
| memory | high_contention | 8 | 75874.38 | 7.9825 | 8.2033 | 8.2033 | 8.2033 |
| memory | high_contention | 16 | 73650.06 | 8.3825 | 8.5216 | 8.5216 | 8.5216 |
| memory | high_contention | 32 | 71922.46 | 8.6839 | 8.8478 | 8.8478 | 8.8478 |
| memory | idle_queue | 1 | 0.00 | 0.0008 | 0.0010 | 0.0010 | 0.0010 |
| memory | idle_queue | 2 | 0.00 | 0.0007 | 0.0008 | 0.0008 | 0.0008 |
| memory | idle_queue | 4 | 0.00 | 0.0013 | 0.0014 | 0.0014 | 0.0014 |
| memory | idle_queue | 8 | 0.00 | 0.0024 | 0.0026 | 0.0026 | 0.0026 |
| memory | idle_queue | 16 | 0.00 | 0.0044 | 0.0046 | 0.0046 | 0.0046 |
| memory | idle_queue | 32 | 0.00 | 0.0092 | 0.0093 | 0.0093 | 0.0093 |
| sqlite | small_jobs | 1 | 1690.01 | 479.5748 | 489.1524 | 489.1524 | 489.1524 |
| sqlite | large_payloads | 1 | 1477.59 | 537.4352 | 537.6851 | 537.6851 | 537.6851 |
| sqlite | batch_jobs | 1 | 1714.12 | 475.9259 | 480.9743 | 480.9743 | 480.9743 |
| sqlite | mixed_priorities | 1 | 1714.15 | 475.8953 | 477.7645 | 477.7645 | 477.7645 |
| sqlite | high_contention | 1 | 1685.40 | 478.7959 | 487.1146 | 487.1146 | 487.1146 |
| sqlite | idle_queue | 1 | 0.00 | 0.2664 | 0.2922 | 0.2922 | 0.2922 |
| sqlite | idle_queue | 2 | 0.00 | 0.5272 | 0.5610 | 0.5610 | 0.5610 |
| sqlite | idle_queue | 4 | 0.00 | 0.8005 | 0.8499 | 0.8499 | 0.8499 |
| sqlite | idle_queue | 8 | 0.00 | 1.4638 | 1.4708 | 1.4708 | 1.4708 |
| sqlite | idle_queue | 16 | 0.00 | 2.8764 | 2.9669 | 2.9669 | 2.9669 |
| sqlite | idle_queue | 32 | 0.00 | 5.6749 | 5.7422 | 5.7422 | 5.7422 |
| postgres | small_jobs | 1 | 275.34 | 1614.4149 | 1757.5252 | 1757.5252 | 1757.5252 |
| postgres | small_jobs | 2 | 299.17 | 1328.7594 | 1341.2461 | 1341.2461 | 1341.2461 |
| postgres | small_jobs | 4 | 316.27 | 1088.2314 | 1154.6156 | 1154.6156 | 1154.6156 |
| postgres | small_jobs | 8 | 299.23 | 1237.0483 | 1314.8116 | 1314.8116 | 1314.8116 |
| postgres | small_jobs | 16 | 284.47 | 1392.3188 | 1418.4792 | 1418.4792 | 1418.4792 |
| postgres | small_jobs | 32 | 272.75 | 1538.9494 | 1557.3990 | 1557.3990 | 1557.3990 |
| postgres | large_payloads | 1 | 193.33 | 2727.6689 | 2790.2430 | 2790.2430 | 2790.2430 |
| postgres | large_payloads | 2 | 224.47 | 1995.6718 | 2065.4396 | 2065.4396 | 2065.4396 |
| postgres | large_payloads | 4 | 244.14 | 1632.5374 | 1637.8561 | 1637.8561 | 1637.8561 |
| postgres | large_payloads | 8 | 234.56 | 1766.3578 | 1843.7645 | 1843.7645 | 1843.7645 |
| postgres | large_payloads | 16 | 236.96 | 1667.4423 | 1835.5901 | 1835.5901 | 1835.5901 |
| postgres | large_payloads | 32 | 237.63 | 1661.6737 | 1707.8691 | 1707.8691 | 1707.8691 |
| postgres | batch_jobs | 1 | 182.36 | 3276.8440 | 3302.3915 | 3302.3915 | 3302.3915 |
| postgres | batch_jobs | 2 | 213.84 | 2354.5573 | 2406.8443 | 2406.8443 | 2406.8443 |
| postgres | batch_jobs | 4 | 236.52 | 1863.7818 | 1882.3099 | 1882.3099 | 1882.3099 |
| postgres | batch_jobs | 8 | 245.89 | 1659.5457 | 1707.7462 | 1707.7462 | 1707.7462 |
| postgres | batch_jobs | 16 | 237.06 | 1814.7709 | 1853.4169 | 1853.4169 | 1853.4169 |
| postgres | batch_jobs | 32 | 230.26 | 1895.5896 | 1936.0979 | 1936.0979 | 1936.0979 |
| postgres | mixed_priorities | 1 | 159.35 | 3815.0266 | 3871.2532 | 3871.2532 | 3871.2532 |
| postgres | mixed_priorities | 2 | 199.85 | 2511.9048 | 2543.4269 | 2543.4269 | 2543.4269 |
| postgres | mixed_priorities | 4 | 227.16 | 1913.4365 | 1918.7580 | 1918.7580 | 1918.7580 |
| postgres | mixed_priorities | 8 | 221.05 | 2009.7591 | 2010.3997 | 2010.3997 | 2010.3997 |
| postgres | mixed_priorities | 16 | 211.04 | 2149.6097 | 2256.3946 | 2256.3946 | 2256.3946 |
| postgres | mixed_priorities | 32 | 212.01 | 2081.1198 | 2244.1434 | 2244.1434 | 2244.1434 |
| postgres | high_contention | 1 | 151.40 | 3916.5831 | 4267.1922 | 4267.1922 | 4267.1922 |
| postgres | high_contention | 2 | 177.53 | 2995.7628 | 3147.1458 | 3147.1458 | 3147.1458 |
| postgres | high_contention | 4 | 199.95 | 2381.6192 | 2542.5769 | 2542.5769 | 2542.5769 |
| postgres | high_contention | 8 | 204.60 | 2196.4917 | 2278.3441 | 2278.3441 | 2278.3441 |
| postgres | high_contention | 16 | 192.57 | 2571.7061 | 2616.9163 | 2616.9163 | 2616.9163 |
| postgres | high_contention | 32 | 185.83 | 2668.1636 | 2764.6769 | 2764.6769 | 2764.6769 |
| postgres | idle_queue | 1 | 0.00 | 208.0803 | 208.6950 | 208.6950 | 208.6950 |
| postgres | idle_queue | 2 | 0.00 | 419.0706 | 419.1528 | 419.1528 | 419.1528 |
| postgres | idle_queue | 4 | 0.00 | 485.9983 | 488.4012 | 488.4012 | 488.4012 |
| postgres | idle_queue | 8 | 0.00 | 627.4046 | 634.1158 | 634.1158 | 634.1158 |
| postgres | idle_queue | 16 | 0.00 | 800.7648 | 809.6458 | 809.6458 | 809.6458 |
| postgres | idle_queue | 32 | 0.00 | 875.5940 | 879.5790 | 879.5790 | 879.5790 |
| redis | small_jobs | 1 | 548.05 | 1209.8831 | 1272.2562 | 1272.2562 | 1272.2562 |
| redis | small_jobs | 2 | 713.52 | 808.8129 | 815.5689 | 815.5689 | 815.5689 |
| redis | small_jobs | 4 | 962.69 | 440.5430 | 440.9010 | 440.9010 | 440.9010 |
| redis | small_jobs | 8 | 1166.78 | 250.3744 | 251.8765 | 251.8765 | 251.8765 |
| redis | small_jobs | 16 | 1341.95 | 147.1451 | 150.6164 | 150.6164 | 150.6164 |
| redis | small_jobs | 32 | 1468.54 | 88.7988 | 90.2555 | 90.2555 | 90.2555 |
| redis | large_payloads | 1 | 511.10 | 1315.8908 | 1320.4741 | 1320.4741 | 1320.4741 |
| redis | large_payloads | 2 | 660.90 | 872.1450 | 885.2719 | 885.2719 | 885.2719 |
| redis | large_payloads | 4 | 874.05 | 502.8244 | 508.2746 | 508.2746 | 508.2746 |
| redis | large_payloads | 8 | 1051.80 | 303.1020 | 322.5439 | 322.5439 | 322.5439 |
| redis | large_payloads | 16 | 1186.80 | 197.0291 | 218.3468 | 218.3468 | 218.3468 |
| redis | large_payloads | 32 | 1285.00 | 138.2462 | 138.9937 | 138.9937 | 138.9937 |
| redis | batch_jobs | 1 | 556.78 | 1193.1161 | 1204.6178 | 1204.6178 | 1204.6178 |
| redis | batch_jobs | 2 | 713.84 | 806.7989 | 811.7993 | 811.7993 | 811.7993 |
| redis | batch_jobs | 4 | 965.12 | 435.1186 | 438.1617 | 438.1617 | 438.1617 |
| redis | batch_jobs | 8 | 1191.76 | 249.5735 | 253.4028 | 253.4028 | 253.4028 |
| redis | batch_jobs | 16 | 1329.96 | 148.4855 | 164.0613 | 164.0613 | 164.0613 |
| redis | batch_jobs | 32 | 1465.11 | 89.1379 | 89.5699 | 89.5699 | 89.5699 |
| redis | mixed_priorities | 1 | 536.30 | 1242.4635 | 1278.7703 | 1278.7703 | 1278.7703 |
| redis | mixed_priorities | 2 | 711.02 | 814.2865 | 818.9314 | 818.9314 | 818.9314 |
| redis | mixed_priorities | 4 | 964.98 | 440.4284 | 441.3532 | 441.3532 | 441.3532 |
| redis | mixed_priorities | 8 | 1169.49 | 252.0127 | 261.7164 | 261.7164 | 261.7164 |
| redis | mixed_priorities | 16 | 1334.73 | 148.9523 | 148.9683 | 148.9683 | 148.9683 |
| redis | mixed_priorities | 32 | 1454.19 | 89.1694 | 89.6653 | 89.6653 | 89.6653 |
| redis | high_contention | 1 | 504.20 | 1182.7053 | 1236.2242 | 1236.2242 | 1236.2242 |
| redis | high_contention | 2 | 625.21 | 806.8633 | 811.5172 | 811.5172 | 811.5172 |
| redis | high_contention | 4 | 813.19 | 439.4125 | 441.4079 | 441.4079 | 441.4079 |
| redis | high_contention | 8 | 956.48 | 251.5691 | 251.6359 | 251.6359 | 251.6359 |
| redis | high_contention | 16 | 1060.18 | 148.1878 | 148.3239 | 148.3239 | 148.3239 |
| redis | high_contention | 32 | 1130.49 | 88.2259 | 88.9571 | 88.9571 | 88.9571 |
| redis | idle_queue | 1 | 0.00 | 0.1564 | 0.1651 | 0.1651 | 0.1651 |
| redis | idle_queue | 2 | 0.00 | 0.3157 | 0.3189 | 0.3189 | 0.3189 |
| redis | idle_queue | 4 | 0.00 | 0.6692 | 0.6924 | 0.6924 | 0.6924 |
| redis | idle_queue | 8 | 0.00 | 1.2671 | 1.2674 | 1.2674 | 1.2674 |
| redis | idle_queue | 16 | 0.00 | 2.6046 | 2.6474 | 2.6474 | 2.6474 |
| redis | idle_queue | 32 | 0.00 | 5.1900 | 5.5454 | 5.5454 | 5.5454 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
