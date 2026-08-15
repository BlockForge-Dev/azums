# Azums M14 Performance Report

- generated_at_unix_ms: 1786835292302
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 172703.19 | 2.5944 | 2.6765 | 2.6765 | 2.6765 |
| memory | small_jobs | 2 | 181472.42 | 3.0476 | 3.1796 | 3.1796 | 3.1796 |
| memory | small_jobs | 4 | 179753.66 | 2.9527 | 3.1016 | 3.1016 | 3.1016 |
| memory | small_jobs | 8 | 164674.84 | 2.8283 | 2.8782 | 2.8782 | 2.8782 |
| memory | small_jobs | 16 | 168059.11 | 2.7301 | 2.8684 | 2.8684 | 2.8684 |
| memory | small_jobs | 32 | 166306.25 | 2.8026 | 2.8903 | 2.8903 | 2.8903 |
| memory | large_payloads | 1 | 89029.24 | 3.2540 | 3.9163 | 3.9163 | 3.9163 |
| memory | large_payloads | 2 | 143727.15 | 3.3148 | 3.7440 | 3.7440 | 3.7440 |
| memory | large_payloads | 4 | 134894.15 | 3.7831 | 4.2098 | 4.2098 | 4.2098 |
| memory | large_payloads | 8 | 122065.07 | 3.7374 | 3.7775 | 3.7775 | 3.7775 |
| memory | large_payloads | 16 | 120536.67 | 4.3639 | 4.4303 | 4.4303 | 4.4303 |
| memory | large_payloads | 32 | 130089.85 | 3.6885 | 3.8548 | 3.8548 | 3.8548 |
| memory | batch_jobs | 1 | 198886.92 | 2.1735 | 2.5231 | 2.5231 | 2.5231 |
| memory | batch_jobs | 2 | 195209.21 | 2.4620 | 2.4721 | 2.4721 | 2.4721 |
| memory | batch_jobs | 4 | 180635.60 | 2.8296 | 2.9849 | 2.9849 | 2.9849 |
| memory | batch_jobs | 8 | 182192.80 | 2.8205 | 2.8628 | 2.8628 | 2.8628 |
| memory | batch_jobs | 16 | 184285.64 | 2.7587 | 2.7614 | 2.7614 | 2.7614 |
| memory | batch_jobs | 32 | 182910.63 | 2.8064 | 2.8874 | 2.8874 | 2.8874 |
| memory | mixed_priorities | 1 | 206378.91 | 2.2325 | 2.2886 | 2.2886 | 2.2886 |
| memory | mixed_priorities | 2 | 195214.44 | 2.4775 | 2.5047 | 2.5047 | 2.5047 |
| memory | mixed_priorities | 4 | 182689.49 | 2.8559 | 2.8586 | 2.8586 | 2.8586 |
| memory | mixed_priorities | 8 | 184893.61 | 2.7913 | 2.8333 | 2.8333 | 2.8333 |
| memory | mixed_priorities | 16 | 186202.64 | 2.7847 | 2.7972 | 2.7972 | 2.7972 |
| memory | mixed_priorities | 32 | 183939.54 | 2.8511 | 2.8693 | 2.8693 | 2.8693 |
| memory | high_contention | 1 | 129423.43 | 2.2469 | 2.3342 | 2.3342 | 2.3342 |
| memory | high_contention | 2 | 125506.81 | 2.5007 | 2.5287 | 2.5287 | 2.5287 |
| memory | high_contention | 4 | 119985.64 | 2.8768 | 2.8891 | 2.8891 | 2.8891 |
| memory | high_contention | 8 | 119369.35 | 2.8886 | 2.9111 | 2.9111 | 2.9111 |
| memory | high_contention | 16 | 119192.19 | 2.8955 | 3.0258 | 3.0258 | 3.0258 |
| memory | high_contention | 32 | 120155.40 | 2.8602 | 2.8742 | 2.8742 | 2.8742 |
| memory | idle_queue | 1 | 0.00 | 0.0004 | 0.0009 | 0.0009 | 0.0009 |
| memory | idle_queue | 2 | 0.00 | 0.0006 | 0.0007 | 0.0007 | 0.0007 |
| memory | idle_queue | 4 | 0.00 | 0.0011 | 0.0012 | 0.0012 | 0.0012 |
| memory | idle_queue | 8 | 0.00 | 0.0020 | 0.0020 | 0.0020 | 0.0020 |
| memory | idle_queue | 16 | 0.00 | 0.0040 | 0.0040 | 0.0040 | 0.0040 |
| memory | idle_queue | 32 | 0.00 | 0.0077 | 0.0078 | 0.0078 | 0.0078 |
| sqlite | small_jobs | 1 | 2198.29 | 370.9336 | 373.9729 | 373.9729 | 373.9729 |
| sqlite | large_payloads | 1 | 1817.91 | 433.9579 | 439.9691 | 439.9691 | 439.9691 |
| sqlite | batch_jobs | 1 | 2144.96 | 383.6119 | 388.8012 | 388.8012 | 388.8012 |
| sqlite | mixed_priorities | 1 | 2143.54 | 383.1219 | 387.3326 | 387.3326 | 387.3326 |
| sqlite | high_contention | 1 | 2138.29 | 384.2320 | 391.0847 | 391.0847 | 391.0847 |
| sqlite | idle_queue | 1 | 0.00 | 0.2442 | 0.2442 | 0.2442 | 0.2442 |
| sqlite | idle_queue | 2 | 0.00 | 0.4738 | 0.4844 | 0.4844 | 0.4844 |
| sqlite | idle_queue | 4 | 0.00 | 0.7354 | 0.8393 | 0.8393 | 0.8393 |
| sqlite | idle_queue | 8 | 0.00 | 1.2735 | 1.2978 | 1.2978 | 1.2978 |
| sqlite | idle_queue | 16 | 0.00 | 2.1260 | 2.3340 | 2.3340 | 2.3340 |
| sqlite | idle_queue | 32 | 0.00 | 4.4975 | 4.5985 | 4.5985 | 4.5985 |
| postgres | small_jobs | 1 | 291.62 | 1527.1597 | 1577.1796 | 1577.1796 | 1577.1796 |
| postgres | small_jobs | 2 | 338.18 | 1158.9770 | 1190.4451 | 1190.4451 | 1190.4451 |
| postgres | small_jobs | 4 | 353.08 | 1014.4045 | 1070.4768 | 1070.4768 | 1070.4768 |
| postgres | small_jobs | 8 | 332.53 | 1133.1320 | 1174.5975 | 1174.5975 | 1174.5975 |
| postgres | small_jobs | 16 | 314.37 | 1287.9568 | 1356.5962 | 1356.5962 | 1356.5962 |
| postgres | small_jobs | 32 | 301.71 | 1386.2024 | 1423.3968 | 1423.3968 | 1423.3968 |
| postgres | large_payloads | 1 | 187.34 | 3497.6099 | 3500.9177 | 3500.9177 | 3500.9177 |
| postgres | large_payloads | 2 | 252.60 | 1785.3097 | 1793.9456 | 1793.9456 | 1793.9456 |
| postgres | large_payloads | 4 | 272.87 | 1484.6341 | 1507.6509 | 1507.6509 | 1507.6509 |
| postgres | large_payloads | 8 | 259.71 | 1593.3883 | 1639.6577 | 1639.6577 | 1639.6577 |
| postgres | large_payloads | 16 | 251.11 | 1701.1117 | 1746.8052 | 1746.8052 | 1746.8052 |
| postgres | large_payloads | 32 | 245.26 | 1848.5098 | 1854.2247 | 1854.2247 | 1854.2247 |
| postgres | batch_jobs | 1 | 203.51 | 2840.6146 | 2878.6680 | 2878.6680 | 2878.6680 |
| postgres | batch_jobs | 2 | 236.34 | 2160.3868 | 2183.7150 | 2183.7150 | 2183.7150 |
| postgres | batch_jobs | 4 | 262.42 | 1687.9221 | 1697.0065 | 1697.0065 | 1697.0065 |
| postgres | batch_jobs | 8 | 254.79 | 1758.3209 | 1803.6491 | 1803.6491 | 1803.6491 |
| postgres | batch_jobs | 16 | 255.03 | 1681.5587 | 1833.3650 | 1833.3650 | 1833.3650 |
| postgres | batch_jobs | 32 | 254.87 | 1705.9903 | 1739.4964 | 1739.4964 | 1739.4964 |
| postgres | mixed_priorities | 1 | 175.06 | 3495.8759 | 3509.8067 | 3509.8067 | 3509.8067 |
| postgres | mixed_priorities | 2 | 208.73 | 2595.9777 | 2623.3100 | 2623.3100 | 2623.3100 |
| postgres | mixed_priorities | 4 | 235.98 | 1941.3862 | 2029.7018 | 2029.7018 | 2029.7018 |
| postgres | mixed_priorities | 8 | 242.66 | 1766.1219 | 1787.9481 | 1787.9481 | 1787.9481 |
| postgres | mixed_priorities | 16 | 235.19 | 1917.3507 | 2005.8195 | 2005.8195 | 2005.8195 |
| postgres | mixed_priorities | 32 | 226.15 | 2097.2465 | 2113.0942 | 2113.0942 | 2113.0942 |
| postgres | high_contention | 1 | 144.28 | 4648.9723 | 4870.7617 | 4870.7617 | 4870.7617 |
| postgres | high_contention | 2 | 193.12 | 2803.4140 | 2814.3111 | 2814.3111 | 2814.3111 |
| postgres | high_contention | 4 | 222.79 | 2100.6257 | 2112.1896 | 2112.1896 | 2112.1896 |
| postgres | high_contention | 8 | 217.67 | 2178.8088 | 2249.7127 | 2249.7127 | 2249.7127 |
| postgres | high_contention | 16 | 201.43 | 2526.5412 | 2589.0036 | 2589.0036 | 2589.0036 |
| postgres | high_contention | 32 | 201.41 | 2472.6152 | 2509.5595 | 2509.5595 | 2509.5595 |
| postgres | idle_queue | 1 | 0.00 | 207.1346 | 207.1460 | 207.1460 | 207.1460 |
| postgres | idle_queue | 2 | 0.00 | 412.3210 | 424.2532 | 424.2532 | 424.2532 |
| postgres | idle_queue | 4 | 0.00 | 487.5462 | 494.4434 | 494.4434 | 494.4434 |
| postgres | idle_queue | 8 | 0.00 | 631.3450 | 631.8342 | 631.8342 | 631.8342 |
| postgres | idle_queue | 16 | 0.00 | 801.1028 | 802.4330 | 802.4330 | 802.4330 |
| postgres | idle_queue | 32 | 0.00 | 896.0504 | 897.1691 | 897.1691 | 897.1691 |
| redis | small_jobs | 1 | 784.69 | 833.5688 | 867.2768 | 867.2768 | 867.2768 |
| redis | small_jobs | 2 | 1021.27 | 564.0929 | 572.7815 | 572.7815 | 572.7815 |
| redis | small_jobs | 4 | 1340.57 | 329.5226 | 329.7109 | 329.7109 | 329.7109 |
| redis | small_jobs | 8 | 1643.29 | 193.7581 | 194.7221 | 194.7221 | 194.7221 |
| redis | small_jobs | 16 | 1875.48 | 115.8059 | 116.8484 | 116.8484 | 116.8484 |
| redis | small_jobs | 32 | 2069.73 | 74.4953 | 76.0730 | 76.0730 | 76.0730 |
| redis | large_payloads | 1 | 719.68 | 932.5911 | 938.2711 | 938.2711 | 938.2711 |
| redis | large_payloads | 2 | 937.13 | 604.3010 | 613.1221 | 613.1221 | 613.1221 |
| redis | large_payloads | 4 | 1156.36 | 402.7225 | 408.6834 | 408.6834 | 408.6834 |
| redis | large_payloads | 8 | 1410.17 | 250.1877 | 251.8870 | 251.8870 | 251.8870 |
| redis | large_payloads | 16 | 1592.17 | 167.7749 | 171.2922 | 171.2922 | 171.2922 |
| redis | large_payloads | 32 | 1709.61 | 127.0735 | 127.4586 | 127.4586 | 127.4586 |
| redis | batch_jobs | 1 | 816.94 | 815.3164 | 815.3526 | 815.3526 | 815.3526 |
| redis | batch_jobs | 2 | 1025.29 | 559.1771 | 565.1552 | 565.1552 | 565.1552 |
| redis | batch_jobs | 4 | 1380.67 | 319.3719 | 319.8512 | 319.8512 | 319.8512 |
| redis | batch_jobs | 8 | 1659.63 | 191.3631 | 191.7515 | 191.7515 | 191.7515 |
| redis | batch_jobs | 16 | 1890.82 | 116.6890 | 118.6983 | 118.6983 | 118.6983 |
| redis | batch_jobs | 32 | 2083.54 | 71.7286 | 71.9141 | 71.9141 | 71.9141 |
| redis | mixed_priorities | 1 | 824.95 | 806.5510 | 811.5425 | 811.5425 | 811.5425 |
| redis | mixed_priorities | 2 | 1041.33 | 553.4906 | 553.5342 | 553.5342 | 553.5342 |
| redis | mixed_priorities | 4 | 1348.00 | 334.4209 | 335.9580 | 335.9580 | 335.9580 |
| redis | mixed_priorities | 8 | 1664.41 | 191.0814 | 198.9969 | 198.9969 | 198.9969 |
| redis | mixed_priorities | 16 | 1856.31 | 121.0766 | 126.1118 | 126.1118 | 126.1118 |
| redis | mixed_priorities | 32 | 2070.14 | 74.2891 | 74.5398 | 74.5398 | 74.5398 |
| redis | high_contention | 1 | 728.87 | 820.0510 | 821.0433 | 821.0433 | 821.0433 |
| redis | high_contention | 2 | 855.69 | 607.1883 | 653.8096 | 653.8096 | 653.8096 |
| redis | high_contention | 4 | 1143.91 | 325.0980 | 329.6854 | 329.6854 | 329.6854 |
| redis | high_contention | 8 | 1344.76 | 193.5691 | 195.3688 | 195.3688 | 195.3688 |
| redis | high_contention | 16 | 1497.25 | 117.1068 | 128.7732 | 128.7732 | 128.7732 |
| redis | high_contention | 32 | 1620.06 | 70.6137 | 71.3581 | 71.3581 | 71.3581 |
| redis | idle_queue | 1 | 0.00 | 0.1234 | 0.1381 | 0.1381 | 0.1381 |
| redis | idle_queue | 2 | 0.00 | 0.2598 | 0.2623 | 0.2623 | 0.2623 |
| redis | idle_queue | 4 | 0.00 | 0.4976 | 0.5123 | 0.5123 | 0.5123 |
| redis | idle_queue | 8 | 0.00 | 0.9589 | 0.9800 | 0.9800 | 0.9800 |
| redis | idle_queue | 16 | 0.00 | 1.8830 | 1.9355 | 1.9355 | 1.9355 |
| redis | idle_queue | 32 | 0.00 | 3.8174 | 3.9383 | 3.9383 | 3.9383 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
