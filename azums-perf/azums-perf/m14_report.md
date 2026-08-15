# Azums M14 Performance Report

- generated_at_unix_ms: 1786790816564
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 199274.20 | 2.3348 | 2.5434 | 2.5434 | 2.5434 |
| memory | small_jobs | 2 | 202248.71 | 2.7028 | 2.8344 | 2.8344 | 2.8344 |
| memory | small_jobs | 4 | 193534.17 | 2.9162 | 3.0118 | 3.0118 | 3.0118 |
| memory | small_jobs | 8 | 192660.29 | 3.0570 | 3.0573 | 3.0573 | 3.0573 |
| memory | small_jobs | 16 | 218830.19 | 2.5844 | 2.8209 | 2.8209 | 2.8209 |
| memory | small_jobs | 32 | 211058.27 | 2.7990 | 2.8818 | 2.8818 | 2.8818 |
| memory | large_payloads | 1 | 99201.91 | 3.1740 | 4.3973 | 4.3973 | 4.3973 |
| memory | large_payloads | 2 | 132017.96 | 3.5131 | 4.1837 | 4.1837 | 4.1837 |
| memory | large_payloads | 4 | 122781.14 | 4.3091 | 4.5569 | 4.5569 | 4.5569 |
| memory | large_payloads | 8 | 130786.53 | 3.9167 | 4.2151 | 4.2151 | 4.2151 |
| memory | large_payloads | 16 | 130574.52 | 3.9183 | 4.2782 | 4.2782 | 4.2782 |
| memory | large_payloads | 32 | 143319.53 | 3.8293 | 3.8806 | 3.8806 | 3.8806 |
| memory | batch_jobs | 1 | 219922.53 | 2.1555 | 2.1633 | 2.1633 | 2.1633 |
| memory | batch_jobs | 2 | 196699.75 | 2.8004 | 2.8263 | 2.8263 | 2.8263 |
| memory | batch_jobs | 4 | 205630.55 | 2.6126 | 2.7625 | 2.7625 | 2.7625 |
| memory | batch_jobs | 8 | 190603.44 | 2.9415 | 3.0847 | 3.0847 | 3.0847 |
| memory | batch_jobs | 16 | 187838.10 | 2.9079 | 3.0084 | 3.0084 | 3.0084 |
| memory | batch_jobs | 32 | 199782.42 | 2.8038 | 2.8686 | 2.8686 | 2.8686 |
| memory | mixed_priorities | 1 | 224493.14 | 2.2298 | 2.2375 | 2.2375 | 2.2375 |
| memory | mixed_priorities | 2 | 197639.32 | 2.8287 | 2.8296 | 2.8296 | 2.8296 |
| memory | mixed_priorities | 4 | 200009.61 | 2.8429 | 2.8793 | 2.8793 | 2.8793 |
| memory | mixed_priorities | 8 | 207752.54 | 2.6926 | 2.8053 | 2.8053 | 2.8053 |
| memory | mixed_priorities | 16 | 209621.60 | 2.8639 | 2.8664 | 2.8664 | 2.8664 |
| memory | mixed_priorities | 32 | 200658.43 | 2.7396 | 2.8718 | 2.8718 | 2.8718 |
| memory | high_contention | 1 | 133545.32 | 1.9974 | 2.0675 | 2.0675 | 2.0675 |
| memory | high_contention | 2 | 125925.72 | 2.5320 | 2.6419 | 2.6419 | 2.6419 |
| memory | high_contention | 4 | 119750.24 | 2.8164 | 2.8674 | 2.8674 | 2.8674 |
| memory | high_contention | 8 | 119121.10 | 2.7151 | 2.7996 | 2.7996 | 2.7996 |
| memory | high_contention | 16 | 122695.06 | 2.7616 | 2.9423 | 2.9423 | 2.9423 |
| memory | high_contention | 32 | 116784.66 | 2.7205 | 2.9218 | 2.9218 | 2.9218 |
| memory | idle_queue | 1 | 0.00 | 0.0009 | 0.0010 | 0.0010 | 0.0010 |
| memory | idle_queue | 2 | 0.00 | 0.0007 | 0.0007 | 0.0007 | 0.0007 |
| memory | idle_queue | 4 | 0.00 | 0.0014 | 0.0015 | 0.0015 | 0.0015 |
| memory | idle_queue | 8 | 0.00 | 0.0024 | 0.0024 | 0.0024 | 0.0024 |
| memory | idle_queue | 16 | 0.00 | 0.0044 | 0.0045 | 0.0045 | 0.0045 |
| memory | idle_queue | 32 | 0.00 | 0.0056 | 0.0083 | 0.0083 | 0.0083 |
| sqlite | small_jobs | 1 | 1866.72 | 427.6729 | 448.2714 | 448.2714 | 448.2714 |
| sqlite | large_payloads | 1 | 1540.48 | 503.5473 | 514.1131 | 514.1131 | 514.1131 |
| sqlite | batch_jobs | 1 | 1812.07 | 455.9568 | 462.3039 | 462.3039 | 462.3039 |
| sqlite | mixed_priorities | 1 | 1775.87 | 461.3868 | 467.7619 | 467.7619 | 467.7619 |
| sqlite | high_contention | 1 | 1797.32 | 445.8779 | 460.4813 | 460.4813 | 460.4813 |
| sqlite | idle_queue | 1 | 0.00 | 0.2543 | 0.2589 | 0.2589 | 0.2589 |
| sqlite | idle_queue | 2 | 0.00 | 0.5060 | 0.5069 | 0.5069 | 0.5069 |
| sqlite | idle_queue | 4 | 0.00 | 0.7267 | 0.8013 | 0.8013 | 0.8013 |
| sqlite | idle_queue | 8 | 0.00 | 1.3232 | 1.3890 | 1.3890 | 1.3890 |
| sqlite | idle_queue | 16 | 0.00 | 2.4312 | 2.4331 | 2.4331 | 2.4331 |
| sqlite | idle_queue | 32 | 0.00 | 4.8881 | 4.9074 | 4.9074 | 4.9074 |
| postgres | small_jobs | 1 | 266.31 | 1741.2923 | 1807.6874 | 1807.6874 | 1807.6874 |
| postgres | small_jobs | 2 | 307.26 | 1230.8766 | 1278.2594 | 1278.2594 | 1278.2594 |
| postgres | small_jobs | 4 | 317.59 | 1106.6514 | 1181.9767 | 1181.9767 | 1181.9767 |
| postgres | small_jobs | 8 | 301.96 | 1241.8593 | 1250.2707 | 1250.2707 | 1250.2707 |
| postgres | small_jobs | 16 | 276.46 | 1397.3559 | 1600.7187 | 1600.7187 | 1600.7187 |
| postgres | small_jobs | 32 | 282.40 | 1499.1898 | 1531.1706 | 1531.1706 | 1531.1706 |
| postgres | large_payloads | 1 | 174.85 | 3319.0505 | 3520.9982 | 3520.9982 | 3520.9982 |
| postgres | large_payloads | 2 | 228.56 | 1974.5989 | 2030.8790 | 2030.8790 | 2030.8790 |
| postgres | large_payloads | 4 | 245.21 | 1563.9549 | 1619.6575 | 1619.6575 | 1619.6575 |
| postgres | large_payloads | 8 | 237.64 | 1711.8544 | 1842.0278 | 1842.0278 | 1842.0278 |
| postgres | large_payloads | 16 | 231.26 | 1879.4445 | 1901.2743 | 1901.2743 | 1901.2743 |
| postgres | large_payloads | 32 | 230.52 | 1701.5761 | 1791.1438 | 1791.1438 | 1791.1438 |
| postgres | batch_jobs | 1 | 184.68 | 3070.4735 | 3236.2224 | 3236.2224 | 3236.2224 |
| postgres | batch_jobs | 2 | 216.08 | 2353.5877 | 2443.4234 | 2443.4234 | 2443.4234 |
| postgres | batch_jobs | 4 | 238.19 | 1855.9803 | 1888.7760 | 1888.7760 | 1888.7760 |
| postgres | batch_jobs | 8 | 236.29 | 1909.9234 | 2066.4717 | 2066.4717 | 2066.4717 |
| postgres | batch_jobs | 16 | 227.43 | 1768.0297 | 1891.6920 | 1891.6920 | 1891.6920 |
| postgres | batch_jobs | 32 | 222.23 | 1943.5500 | 1949.2622 | 1949.2622 | 1949.2622 |
| postgres | mixed_priorities | 1 | 153.66 | 4100.6324 | 4182.8368 | 4182.8368 | 4182.8368 |
| postgres | mixed_priorities | 2 | 184.65 | 2715.0074 | 2834.0278 | 2834.0278 | 2834.0278 |
| postgres | mixed_priorities | 4 | 228.47 | 1891.9162 | 1954.8312 | 1954.8312 | 1954.8312 |
| postgres | mixed_priorities | 8 | 206.71 | 2087.8863 | 2184.6065 | 2184.6065 | 2184.6065 |
| postgres | mixed_priorities | 16 | 210.30 | 2193.2638 | 2309.2343 | 2309.2343 | 2309.2343 |
| postgres | mixed_priorities | 32 | 203.30 | 2345.1702 | 2376.0676 | 2376.0676 | 2376.0676 |
| postgres | high_contention | 1 | 152.44 | 4013.0702 | 4155.8289 | 4155.8289 | 4155.8289 |
| postgres | high_contention | 2 | 171.15 | 3326.6013 | 3433.5778 | 3433.5778 | 3433.5778 |
| postgres | high_contention | 4 | 196.71 | 2530.5286 | 2550.2093 | 2550.2093 | 2550.2093 |
| postgres | high_contention | 8 | 198.05 | 2471.6170 | 2475.5651 | 2475.5651 | 2475.5651 |
| postgres | high_contention | 16 | 192.94 | 2491.0685 | 2642.8594 | 2642.8594 | 2642.8594 |
| postgres | high_contention | 32 | 174.58 | 2712.6586 | 3516.1070 | 3516.1070 | 3516.1070 |
| postgres | idle_queue | 1 | 0.00 | 213.6676 | 214.3580 | 214.3580 | 214.3580 |
| postgres | idle_queue | 2 | 0.00 | 432.5361 | 443.0061 | 443.0061 | 443.0061 |
| postgres | idle_queue | 4 | 0.00 | 523.5701 | 525.0665 | 525.0665 | 525.0665 |
| postgres | idle_queue | 8 | 0.00 | 660.9451 | 661.9463 | 661.9463 | 661.9463 |
| postgres | idle_queue | 16 | 0.00 | 822.6836 | 843.1887 | 843.1887 | 843.1887 |
| postgres | idle_queue | 32 | 0.00 | 930.2309 | 932.2547 | 932.2547 | 932.2547 |
| redis | small_jobs | 1 | 596.72 | 1104.6644 | 1154.8202 | 1154.8202 | 1154.8202 |
| redis | small_jobs | 2 | 788.77 | 726.9671 | 733.3677 | 733.3677 | 733.3677 |
| redis | small_jobs | 4 | 1057.30 | 407.0297 | 412.7524 | 412.7524 | 412.7524 |
| redis | small_jobs | 8 | 1298.34 | 234.8084 | 237.8130 | 237.8130 | 237.8130 |
| redis | small_jobs | 16 | 1465.38 | 140.8796 | 143.3940 | 143.3940 | 143.3940 |
| redis | small_jobs | 32 | 1599.28 | 89.6466 | 93.3222 | 93.3222 | 93.3222 |
| redis | large_payloads | 1 | 542.49 | 1243.7544 | 1252.5738 | 1252.5738 | 1252.5738 |
| redis | large_payloads | 2 | 714.82 | 795.7632 | 807.6809 | 807.6809 | 807.6809 |
| redis | large_payloads | 4 | 906.25 | 490.6182 | 511.8556 | 511.8556 | 511.8556 |
| redis | large_payloads | 8 | 1116.26 | 298.0316 | 298.1601 | 298.1601 | 298.1601 |
| redis | large_payloads | 16 | 1248.14 | 199.5086 | 204.7446 | 204.7446 | 204.7446 |
| redis | large_payloads | 32 | 1338.47 | 144.6582 | 145.4745 | 145.4745 | 145.4745 |
| redis | batch_jobs | 1 | 612.38 | 1091.4903 | 1103.0620 | 1103.0620 | 1103.0620 |
| redis | batch_jobs | 2 | 786.02 | 731.1707 | 732.1774 | 732.1774 | 732.1774 |
| redis | batch_jobs | 4 | 1057.29 | 403.0134 | 403.3006 | 403.3006 | 403.3006 |
| redis | batch_jobs | 8 | 1290.43 | 238.2121 | 240.7201 | 240.7201 | 240.7201 |
| redis | batch_jobs | 16 | 1462.88 | 141.4301 | 141.6885 | 141.6885 | 141.6885 |
| redis | batch_jobs | 32 | 1595.17 | 85.9899 | 87.8055 | 87.8055 | 87.8055 |
| redis | mixed_priorities | 1 | 608.10 | 1097.1020 | 1152.3315 | 1152.3315 | 1152.3315 |
| redis | mixed_priorities | 2 | 745.86 | 804.4185 | 823.1929 | 823.1929 | 823.1929 |
| redis | mixed_priorities | 4 | 1055.43 | 405.7650 | 406.2386 | 406.2386 | 406.2386 |
| redis | mixed_priorities | 8 | 1269.58 | 236.6993 | 242.6724 | 242.6724 | 242.6724 |
| redis | mixed_priorities | 16 | 1448.94 | 149.1611 | 160.8378 | 160.8378 | 160.8378 |
| redis | mixed_priorities | 32 | 1596.49 | 89.8795 | 91.4673 | 91.4673 | 91.4673 |
| redis | high_contention | 1 | 554.00 | 1096.5521 | 1097.1438 | 1097.1438 | 1097.1438 |
| redis | high_contention | 2 | 688.83 | 732.6158 | 741.4586 | 741.4586 | 741.4586 |
| redis | high_contention | 4 | 887.90 | 406.6647 | 407.9798 | 407.9798 | 407.9798 |
| redis | high_contention | 8 | 1054.49 | 233.7852 | 234.1326 | 234.1326 | 234.1326 |
| redis | high_contention | 16 | 1181.33 | 140.2973 | 142.8225 | 142.8225 | 142.8225 |
| redis | high_contention | 32 | 1238.07 | 86.7498 | 87.0925 | 87.0925 | 87.0925 |
| redis | idle_queue | 1 | 0.00 | 0.1629 | 0.1665 | 0.1665 | 0.1665 |
| redis | idle_queue | 2 | 0.00 | 0.3184 | 0.3725 | 0.3725 | 0.3725 |
| redis | idle_queue | 4 | 0.00 | 0.5908 | 0.5963 | 0.5963 | 0.5963 |
| redis | idle_queue | 8 | 0.00 | 1.1542 | 1.1588 | 1.1588 | 1.1588 |
| redis | idle_queue | 16 | 0.00 | 2.3544 | 2.4069 | 2.4069 | 2.4069 |
| redis | idle_queue | 32 | 0.00 | 5.1076 | 5.1384 | 5.1384 | 5.1384 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
