# Azums M14 Performance Report

- generated_at_unix_ms: 1787379460899
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 207557.16 | 2.1457 | 2.3647 | 2.3647 | 2.3647 |
| memory | small_jobs | 2 | 220369.23 | 2.4813 | 2.5512 | 2.5512 | 2.5512 |
| memory | small_jobs | 4 | 209326.11 | 2.5776 | 2.7043 | 2.7043 | 2.7043 |
| memory | small_jobs | 8 | 190684.43 | 2.5634 | 2.5907 | 2.5907 | 2.5907 |
| memory | small_jobs | 16 | 189751.74 | 2.5892 | 2.6449 | 2.6449 | 2.6449 |
| memory | small_jobs | 32 | 189268.06 | 2.6435 | 2.6485 | 2.6485 | 2.6485 |
| memory | large_payloads | 1 | 103021.77 | 3.2498 | 3.9623 | 3.9623 | 3.9623 |
| memory | large_payloads | 2 | 139804.97 | 3.6209 | 3.6933 | 3.6933 | 3.6933 |
| memory | large_payloads | 4 | 140612.66 | 3.8421 | 4.2268 | 4.2268 | 4.2268 |
| memory | large_payloads | 8 | 137020.04 | 3.8992 | 4.2293 | 4.2293 | 4.2293 |
| memory | large_payloads | 16 | 141903.60 | 3.6757 | 3.8343 | 3.8343 | 3.8343 |
| memory | large_payloads | 32 | 143096.39 | 3.7237 | 3.8985 | 3.8985 | 3.8985 |
| memory | batch_jobs | 1 | 232256.93 | 2.0321 | 2.1260 | 2.1260 | 2.1260 |
| memory | batch_jobs | 2 | 224140.92 | 2.2917 | 2.3153 | 2.3153 | 2.3153 |
| memory | batch_jobs | 4 | 211162.60 | 2.5841 | 2.6018 | 2.6018 | 2.6018 |
| memory | batch_jobs | 8 | 210893.24 | 2.5713 | 2.6030 | 2.6030 | 2.6030 |
| memory | batch_jobs | 16 | 213131.03 | 2.5075 | 2.6463 | 2.6463 | 2.6463 |
| memory | batch_jobs | 32 | 208067.45 | 2.6064 | 2.6923 | 2.6923 | 2.6923 |
| memory | mixed_priorities | 1 | 229494.79 | 2.2021 | 2.2626 | 2.2626 | 2.2626 |
| memory | mixed_priorities | 2 | 202619.04 | 2.7740 | 2.8085 | 2.8085 | 2.8085 |
| memory | mixed_priorities | 4 | 208560.14 | 2.6188 | 2.8054 | 2.8054 | 2.8054 |
| memory | mixed_priorities | 8 | 207648.35 | 2.6764 | 2.7375 | 2.7375 | 2.7375 |
| memory | mixed_priorities | 16 | 212426.30 | 2.6097 | 2.6723 | 2.6723 | 2.6723 |
| memory | mixed_priorities | 32 | 212227.74 | 2.5727 | 2.5963 | 2.5963 | 2.5963 |
| memory | high_contention | 1 | 127335.27 | 2.2338 | 2.3256 | 2.3256 | 2.3256 |
| memory | high_contention | 2 | 119125.41 | 2.8163 | 2.8874 | 2.8874 | 2.8874 |
| memory | high_contention | 4 | 120362.83 | 2.7600 | 2.7750 | 2.7750 | 2.7750 |
| memory | high_contention | 8 | 122842.76 | 2.7138 | 2.7154 | 2.7154 | 2.7154 |
| memory | high_contention | 16 | 121367.69 | 2.7481 | 2.7873 | 2.7873 | 2.7873 |
| memory | high_contention | 32 | 121353.73 | 2.7951 | 2.8004 | 2.8004 | 2.8004 |
| memory | idle_queue | 1 | 0.00 | 0.0006 | 0.0013 | 0.0013 | 0.0013 |
| memory | idle_queue | 2 | 0.00 | 0.0008 | 0.0009 | 0.0009 | 0.0009 |
| memory | idle_queue | 4 | 0.00 | 0.0008 | 0.0013 | 0.0013 | 0.0013 |
| memory | idle_queue | 8 | 0.00 | 0.0015 | 0.0025 | 0.0025 | 0.0025 |
| memory | idle_queue | 16 | 0.00 | 0.0028 | 0.0046 | 0.0046 | 0.0046 |
| memory | idle_queue | 32 | 0.00 | 0.0055 | 0.0089 | 0.0089 | 0.0089 |
| sqlite | small_jobs | 1 | 1880.64 | 429.7368 | 437.8175 | 437.8175 | 437.8175 |
| sqlite | large_payloads | 1 | 1601.76 | 501.2583 | 501.9479 | 501.9479 | 501.9479 |
| sqlite | batch_jobs | 1 | 1904.37 | 426.3992 | 428.5993 | 428.5993 | 428.5993 |
| sqlite | mixed_priorities | 1 | 1849.31 | 441.0688 | 450.2725 | 450.2725 | 450.2725 |
| sqlite | high_contention | 1 | 1840.20 | 441.2779 | 446.4656 | 446.4656 | 446.4656 |
| sqlite | idle_queue | 1 | 0.00 | 0.2395 | 0.2516 | 0.2516 | 0.2516 |
| sqlite | idle_queue | 2 | 0.00 | 0.4576 | 0.4590 | 0.4590 | 0.4590 |
| sqlite | idle_queue | 4 | 0.00 | 0.7317 | 0.7674 | 0.7674 | 0.7674 |
| sqlite | idle_queue | 8 | 0.00 | 1.2933 | 1.3358 | 1.3358 | 1.3358 |
| sqlite | idle_queue | 16 | 0.00 | 2.5736 | 2.6241 | 2.6241 | 2.6241 |
| sqlite | idle_queue | 32 | 0.00 | 4.6514 | 4.9841 | 4.9841 | 4.9841 |
| postgres | small_jobs | 1 | 256.97 | 1722.5387 | 1803.3345 | 1803.3345 | 1803.3345 |
| postgres | small_jobs | 2 | 299.58 | 1243.4587 | 1349.0417 | 1349.0417 | 1349.0417 |
| postgres | small_jobs | 4 | 313.11 | 1106.5828 | 1138.0631 | 1138.0631 | 1138.0631 |
| postgres | small_jobs | 8 | 297.22 | 1263.1850 | 1315.4055 | 1315.4055 | 1315.4055 |
| postgres | small_jobs | 16 | 283.03 | 1421.4508 | 1454.5812 | 1454.5812 | 1454.5812 |
| postgres | small_jobs | 32 | 271.15 | 1520.9860 | 1576.6743 | 1576.6743 | 1576.6743 |
| postgres | large_payloads | 1 | 180.41 | 2873.9940 | 3691.9775 | 3691.9775 | 3691.9775 |
| postgres | large_payloads | 2 | 221.20 | 2048.5930 | 2116.7314 | 2116.7314 | 2116.7314 |
| postgres | large_payloads | 4 | 241.07 | 1661.9233 | 1683.9106 | 1683.9106 | 1683.9106 |
| postgres | large_payloads | 8 | 231.66 | 1797.8241 | 1833.9490 | 1833.9490 | 1833.9490 |
| postgres | large_payloads | 16 | 225.77 | 1861.2065 | 1956.1970 | 1956.1970 | 1956.1970 |
| postgres | large_payloads | 32 | 228.46 | 1775.2882 | 1783.8605 | 1783.8605 | 1783.8605 |
| postgres | batch_jobs | 1 | 176.83 | 3216.2343 | 3522.7577 | 3522.7577 | 3522.7577 |
| postgres | batch_jobs | 2 | 208.96 | 2378.9938 | 2467.1767 | 2467.1767 | 2467.1767 |
| postgres | batch_jobs | 4 | 233.63 | 1853.7210 | 2100.3207 | 2100.3207 | 2100.3207 |
| postgres | batch_jobs | 8 | 246.43 | 1637.1550 | 1717.9378 | 1717.9378 | 1717.9378 |
| postgres | batch_jobs | 16 | 233.66 | 1852.8993 | 1871.6532 | 1871.6532 | 1871.6532 |
| postgres | batch_jobs | 32 | 224.80 | 1948.0936 | 2013.1438 | 2013.1438 | 2013.1438 |
| postgres | mixed_priorities | 1 | 153.93 | 3984.6939 | 4126.2677 | 4126.2677 | 4126.2677 |
| postgres | mixed_priorities | 2 | 199.94 | 2466.9903 | 2485.5296 | 2485.5296 | 2485.5296 |
| postgres | mixed_priorities | 4 | 220.11 | 1903.6628 | 1993.7491 | 1993.7491 | 1993.7491 |
| postgres | mixed_priorities | 8 | 215.87 | 2045.8706 | 2057.1451 | 2057.1451 | 2057.1451 |
| postgres | mixed_priorities | 16 | 211.54 | 2163.2689 | 2194.0296 | 2194.0296 | 2194.0296 |
| postgres | mixed_priorities | 32 | 209.16 | 2066.5499 | 2367.3703 | 2367.3703 | 2367.3703 |
| postgres | high_contention | 1 | 151.90 | 3927.1871 | 4012.8362 | 4012.8362 | 4012.8362 |
| postgres | high_contention | 2 | 176.38 | 3051.9793 | 3095.1299 | 3095.1299 | 3095.1299 |
| postgres | high_contention | 4 | 199.66 | 2295.6307 | 2485.8643 | 2485.8643 | 2485.8643 |
| postgres | high_contention | 8 | 203.66 | 2234.7514 | 2294.8068 | 2294.8068 | 2294.8068 |
| postgres | high_contention | 16 | 193.68 | 2459.9137 | 2546.3015 | 2546.3015 | 2546.3015 |
| postgres | high_contention | 32 | 184.04 | 2654.2376 | 2812.2424 | 2812.2424 | 2812.2424 |
| postgres | idle_queue | 1 | 0.00 | 207.1275 | 208.0480 | 208.0480 | 208.0480 |
| postgres | idle_queue | 2 | 0.00 | 417.5782 | 418.6190 | 418.6190 | 418.6190 |
| postgres | idle_queue | 4 | 0.00 | 485.7892 | 488.7622 | 488.7622 | 488.7622 |
| postgres | idle_queue | 8 | 0.00 | 627.0035 | 631.0518 | 631.0518 | 631.0518 |
| postgres | idle_queue | 16 | 0.00 | 801.7682 | 802.0343 | 802.0343 | 802.0343 |
| postgres | idle_queue | 32 | 0.00 | 883.2337 | 896.0784 | 896.0784 | 896.0784 |
| redis | small_jobs | 1 | 598.92 | 1087.5535 | 1190.1902 | 1190.1902 | 1190.1902 |
| redis | small_jobs | 2 | 777.88 | 737.4599 | 749.7181 | 749.7181 | 749.7181 |
| redis | small_jobs | 4 | 1062.53 | 409.9388 | 421.2747 | 421.2747 | 421.2747 |
| redis | small_jobs | 8 | 1306.42 | 230.9792 | 232.2813 | 232.2813 | 232.2813 |
| redis | small_jobs | 16 | 1488.67 | 138.7583 | 139.1870 | 139.1870 | 139.1870 |
| redis | small_jobs | 32 | 1606.18 | 86.1659 | 88.1365 | 88.1365 | 88.1365 |
| redis | large_payloads | 1 | 563.34 | 1188.8408 | 1230.0622 | 1230.0622 | 1230.0622 |
| redis | large_payloads | 2 | 721.70 | 794.4599 | 796.1961 | 796.1961 | 796.1961 |
| redis | large_payloads | 4 | 936.36 | 478.9461 | 489.1006 | 489.1006 | 489.1006 |
| redis | large_payloads | 8 | 1147.39 | 290.0809 | 290.6246 | 290.6246 | 290.6246 |
| redis | large_payloads | 16 | 1281.17 | 192.7651 | 196.1565 | 196.1565 | 196.1565 |
| redis | large_payloads | 32 | 1374.32 | 139.8949 | 140.6672 | 140.6672 | 140.6672 |
| redis | batch_jobs | 1 | 617.84 | 1083.7057 | 1084.0181 | 1084.0181 | 1084.0181 |
| redis | batch_jobs | 2 | 787.91 | 735.0607 | 737.1236 | 737.1236 | 737.1236 |
| redis | batch_jobs | 4 | 1065.88 | 405.7701 | 406.1785 | 406.1785 | 406.1785 |
| redis | batch_jobs | 8 | 1291.76 | 232.0542 | 233.0975 | 233.0975 | 233.0975 |
| redis | batch_jobs | 16 | 1498.29 | 138.7282 | 140.4749 | 140.4749 | 140.4749 |
| redis | batch_jobs | 32 | 1590.53 | 85.0958 | 85.6994 | 85.6994 | 85.6994 |
| redis | mixed_priorities | 1 | 614.88 | 1109.2188 | 1112.9498 | 1112.9498 | 1112.9498 |
| redis | mixed_priorities | 2 | 745.37 | 777.7295 | 851.8959 | 851.8959 | 851.8959 |
| redis | mixed_priorities | 4 | 1062.50 | 406.1849 | 406.6783 | 406.6783 | 406.6783 |
| redis | mixed_priorities | 8 | 1289.51 | 241.7410 | 247.9706 | 247.9706 | 247.9706 |
| redis | mixed_priorities | 16 | 1451.63 | 142.9508 | 149.9131 | 149.9131 | 149.9131 |
| redis | mixed_priorities | 32 | 1609.26 | 86.7808 | 88.8761 | 88.8761 | 88.8761 |
| redis | high_contention | 1 | 559.95 | 1066.5179 | 1112.4131 | 1112.4131 | 1112.4131 |
| redis | high_contention | 2 | 690.09 | 736.2448 | 746.7909 | 746.7909 | 746.7909 |
| redis | high_contention | 4 | 905.29 | 407.3567 | 407.7990 | 407.7990 | 407.7990 |
| redis | high_contention | 8 | 1057.47 | 232.6560 | 234.7373 | 234.7373 | 234.7373 |
| redis | high_contention | 16 | 1161.13 | 139.6995 | 139.8372 | 139.8372 | 139.8372 |
| redis | high_contention | 32 | 1255.52 | 84.0592 | 86.6216 | 86.6216 | 86.6216 |
| redis | idle_queue | 1 | 0.00 | 0.1507 | 0.1836 | 0.1836 | 0.1836 |
| redis | idle_queue | 2 | 0.00 | 0.2831 | 0.3104 | 0.3104 | 0.3104 |
| redis | idle_queue | 4 | 0.00 | 0.6186 | 0.6525 | 0.6525 | 0.6525 |
| redis | idle_queue | 8 | 0.00 | 1.1650 | 1.1800 | 1.1800 | 1.1800 |
| redis | idle_queue | 16 | 0.00 | 2.3435 | 2.6016 | 2.6016 | 2.6016 |
| redis | idle_queue | 32 | 0.00 | 4.7925 | 5.2301 | 5.2301 | 5.2301 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
