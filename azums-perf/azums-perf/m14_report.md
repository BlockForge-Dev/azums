# Azums M14 Performance Report

- generated_at_unix_ms: 1787438120840
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 171254.95 | 2.6137 | 3.2396 | 3.2396 | 3.2396 |
| memory | small_jobs | 2 | 188368.54 | 2.8283 | 2.8782 | 2.8782 | 2.8782 |
| memory | small_jobs | 4 | 168020.70 | 3.3983 | 3.5017 | 3.5017 | 3.5017 |
| memory | small_jobs | 8 | 169049.58 | 3.4131 | 3.4650 | 3.4650 | 3.4650 |
| memory | small_jobs | 16 | 177013.54 | 3.1993 | 3.2640 | 3.2640 | 3.2640 |
| memory | small_jobs | 32 | 176845.60 | 3.2150 | 3.4190 | 3.4190 | 3.4190 |
| memory | large_payloads | 1 | 68002.59 | 5.6561 | 5.7241 | 5.7241 | 5.7241 |
| memory | large_payloads | 2 | 90159.84 | 5.5257 | 5.6958 | 5.6958 | 5.6958 |
| memory | large_payloads | 4 | 78663.29 | 6.3683 | 6.8319 | 6.8319 | 6.8319 |
| memory | large_payloads | 8 | 71591.24 | 6.7474 | 6.7558 | 6.7558 | 6.7558 |
| memory | large_payloads | 16 | 84249.84 | 6.2398 | 6.4593 | 6.4593 | 6.4593 |
| memory | large_payloads | 32 | 82455.82 | 6.1632 | 6.8288 | 6.8288 | 6.8288 |
| memory | batch_jobs | 1 | 184508.42 | 2.4256 | 2.6631 | 2.6631 | 2.6631 |
| memory | batch_jobs | 2 | 189688.89 | 2.6957 | 2.7961 | 2.7961 | 2.7961 |
| memory | batch_jobs | 4 | 169726.27 | 3.4707 | 3.5448 | 3.5448 | 3.5448 |
| memory | batch_jobs | 8 | 174907.50 | 3.1686 | 3.4706 | 3.4706 | 3.4706 |
| memory | batch_jobs | 16 | 177749.01 | 3.1247 | 3.1855 | 3.1855 | 3.1855 |
| memory | batch_jobs | 32 | 176306.90 | 3.1580 | 3.2454 | 3.2454 | 3.2454 |
| memory | mixed_priorities | 1 | 205729.07 | 2.3778 | 2.7189 | 2.7189 | 2.7189 |
| memory | mixed_priorities | 2 | 180198.78 | 3.0773 | 3.3059 | 3.3059 | 3.3059 |
| memory | mixed_priorities | 4 | 172522.66 | 3.3267 | 3.4021 | 3.4021 | 3.4021 |
| memory | mixed_priorities | 8 | 176940.40 | 3.1813 | 3.2808 | 3.2808 | 3.2808 |
| memory | mixed_priorities | 16 | 176928.33 | 3.2502 | 3.3291 | 3.3291 | 3.3291 |
| memory | mixed_priorities | 32 | 177131.16 | 3.1778 | 3.1963 | 3.1963 | 3.1963 |
| memory | high_contention | 1 | 112919.12 | 2.7420 | 2.9713 | 2.9713 | 2.9713 |
| memory | high_contention | 2 | 116036.35 | 2.8344 | 2.8921 | 2.8921 | 2.8921 |
| memory | high_contention | 4 | 108566.23 | 3.2797 | 3.3926 | 3.3926 | 3.3926 |
| memory | high_contention | 8 | 104401.42 | 3.5167 | 3.5754 | 3.5754 | 3.5754 |
| memory | high_contention | 16 | 109581.05 | 3.1695 | 3.2172 | 3.2172 | 3.2172 |
| memory | high_contention | 32 | 110372.55 | 3.3382 | 3.3695 | 3.3695 | 3.3695 |
| memory | idle_queue | 1 | 0.00 | 0.0005 | 0.0012 | 0.0012 | 0.0012 |
| memory | idle_queue | 2 | 0.00 | 0.0007 | 0.0008 | 0.0008 | 0.0008 |
| memory | idle_queue | 4 | 0.00 | 0.0014 | 0.0018 | 0.0018 | 0.0018 |
| memory | idle_queue | 8 | 0.00 | 0.0027 | 0.0028 | 0.0028 | 0.0028 |
| memory | idle_queue | 16 | 0.00 | 0.0029 | 0.0047 | 0.0047 | 0.0047 |
| memory | idle_queue | 32 | 0.00 | 0.0056 | 0.0093 | 0.0093 | 0.0093 |
| sqlite | small_jobs | 1 | 1675.69 | 498.9646 | 501.0525 | 501.0525 | 501.0525 |
| sqlite | large_payloads | 1 | 1433.32 | 552.0126 | 573.6251 | 573.6251 | 573.6251 |
| sqlite | batch_jobs | 1 | 1709.71 | 477.8342 | 484.1143 | 484.1143 | 484.1143 |
| sqlite | mixed_priorities | 1 | 1604.97 | 510.0791 | 534.8241 | 534.8241 | 534.8241 |
| sqlite | high_contention | 1 | 1635.55 | 494.0672 | 515.8838 | 515.8838 | 515.8838 |
| sqlite | idle_queue | 1 | 0.00 | 0.2896 | 0.2929 | 0.2929 | 0.2929 |
| sqlite | idle_queue | 2 | 0.00 | 0.5271 | 0.5897 | 0.5897 | 0.5897 |
| sqlite | idle_queue | 4 | 0.00 | 0.8682 | 0.8759 | 0.8759 | 0.8759 |
| sqlite | idle_queue | 8 | 0.00 | 1.4514 | 1.4639 | 1.4639 | 1.4639 |
| sqlite | idle_queue | 16 | 0.00 | 2.8342 | 2.8426 | 2.8426 | 2.8426 |
| sqlite | idle_queue | 32 | 0.00 | 5.4742 | 5.6449 | 5.6449 | 5.6449 |
| postgres | small_jobs | 1 | 204.05 | 2335.4059 | 2363.5624 | 2363.5624 | 2363.5624 |
| postgres | small_jobs | 2 | 260.87 | 1487.0682 | 1671.7122 | 1671.7122 | 1671.7122 |
| postgres | small_jobs | 4 | 253.41 | 1392.0670 | 1772.5912 | 1772.5912 | 1772.5912 |
| postgres | small_jobs | 8 | 258.01 | 1532.9094 | 1611.6526 | 1611.6526 | 1611.6526 |
| postgres | small_jobs | 16 | 223.56 | 2076.4309 | 2096.8481 | 2096.8481 | 2096.8481 |
| postgres | small_jobs | 32 | 230.94 | 1383.4924 | 1656.9159 | 1656.9159 | 1656.9159 |
| postgres | large_payloads | 1 | 157.44 | 3372.9118 | 3652.4678 | 3652.4678 | 3652.4678 |
| postgres | large_payloads | 2 | 178.67 | 2793.6474 | 2892.3650 | 2892.3650 | 2892.3650 |
| postgres | large_payloads | 4 | 194.91 | 2123.5527 | 2240.2367 | 2240.2367 | 2240.2367 |
| postgres | large_payloads | 8 | 207.38 | 1838.8689 | 2105.1702 | 2105.1702 | 2105.1702 |
| postgres | large_payloads | 16 | 204.71 | 1925.2098 | 2002.6745 | 2002.6745 | 2002.6745 |
| postgres | large_payloads | 32 | 193.03 | 2129.6444 | 2131.3714 | 2131.3714 | 2131.3714 |
| postgres | batch_jobs | 1 | 141.85 | 4383.6638 | 4828.5295 | 4828.5295 | 4828.5295 |
| postgres | batch_jobs | 2 | 187.44 | 2748.9132 | 2843.3066 | 2843.3066 | 2843.3066 |
| postgres | batch_jobs | 4 | 205.80 | 2070.3660 | 2096.3461 | 2096.3461 | 2096.3461 |
| postgres | batch_jobs | 8 | 196.66 | 2166.8951 | 2529.8044 | 2529.8044 | 2529.8044 |
| postgres | batch_jobs | 16 | 186.47 | 2507.3080 | 2515.5845 | 2515.5845 | 2515.5845 |
| postgres | batch_jobs | 32 | 198.61 | 2174.1441 | 2176.0737 | 2176.0737 | 2176.0737 |
| postgres | mixed_priorities | 1 | 134.72 | 4624.0414 | 5111.1846 | 5111.1846 | 5111.1846 |
| postgres | mixed_priorities | 2 | 153.45 | 3644.9253 | 3954.9928 | 3954.9928 | 3954.9928 |
| postgres | mixed_priorities | 4 | 191.22 | 2342.9239 | 2386.2745 | 2386.2745 | 2386.2745 |
| postgres | mixed_priorities | 8 | 178.09 | 2568.9041 | 2901.3347 | 2901.3347 | 2901.3347 |
| postgres | mixed_priorities | 16 | 181.42 | 2583.3608 | 2639.7674 | 2639.7674 | 2639.7674 |
| postgres | mixed_priorities | 32 | 174.20 | 2764.4196 | 2960.7977 | 2960.7977 | 2960.7977 |
| postgres | high_contention | 1 | 122.19 | 5479.8260 | 5512.8773 | 5512.8773 | 5512.8773 |
| postgres | high_contention | 2 | 137.89 | 4225.2896 | 4318.7579 | 4318.7579 | 4318.7579 |
| postgres | high_contention | 4 | 170.29 | 2766.7771 | 3018.7124 | 3018.7124 | 3018.7124 |
| postgres | high_contention | 8 | 166.68 | 2927.3932 | 3039.7291 | 3039.7291 | 3039.7291 |
| postgres | high_contention | 16 | 150.55 | 3379.9500 | 3816.5488 | 3816.5488 | 3816.5488 |
| postgres | high_contention | 32 | 145.52 | 3424.6944 | 3739.3427 | 3739.3427 | 3739.3427 |
| postgres | idle_queue | 1 | 0.00 | 233.5625 | 237.0464 | 237.0464 | 237.0464 |
| postgres | idle_queue | 2 | 0.00 | 467.9517 | 486.1387 | 486.1387 | 486.1387 |
| postgres | idle_queue | 4 | 0.00 | 580.9046 | 600.4217 | 600.4217 | 600.4217 |
| postgres | idle_queue | 8 | 0.00 | 775.9350 | 793.7341 | 793.7341 | 793.7341 |
| postgres | idle_queue | 16 | 0.00 | 991.3968 | 1023.2647 | 1023.2647 | 1023.2647 |
| postgres | idle_queue | 32 | 0.00 | 1128.5622 | 1133.1871 | 1133.1871 | 1133.1871 |
| redis | small_jobs | 1 | 557.00 | 1224.0093 | 1246.2684 | 1246.2684 | 1246.2684 |
| redis | small_jobs | 2 | 741.72 | 776.6142 | 790.5757 | 790.5757 | 790.5757 |
| redis | small_jobs | 4 | 986.01 | 433.6050 | 445.1688 | 445.1688 | 445.1688 |
| redis | small_jobs | 8 | 1203.10 | 253.5378 | 254.7117 | 254.7117 | 254.7117 |
| redis | small_jobs | 16 | 1376.38 | 152.6974 | 153.2657 | 153.2657 | 153.2657 |
| redis | small_jobs | 32 | 1489.17 | 97.0765 | 100.4947 | 100.4947 | 100.4947 |
| redis | large_payloads | 1 | 507.11 | 1323.9696 | 1324.9683 | 1324.9683 | 1324.9683 |
| redis | large_payloads | 2 | 670.26 | 857.1515 | 860.3317 | 860.3317 | 860.3317 |
| redis | large_payloads | 4 | 853.35 | 531.8461 | 536.0843 | 536.0843 | 536.0843 |
| redis | large_payloads | 8 | 1046.13 | 319.4798 | 323.5807 | 323.5807 | 323.5807 |
| redis | large_payloads | 16 | 1191.69 | 208.7181 | 217.8735 | 217.8735 | 217.8735 |
| redis | large_payloads | 32 | 1254.33 | 158.0506 | 162.6459 | 162.6459 | 162.6459 |
| redis | batch_jobs | 1 | 572.27 | 1162.7885 | 1182.2514 | 1182.2514 | 1182.2514 |
| redis | batch_jobs | 2 | 740.31 | 771.6314 | 789.3075 | 789.3075 | 789.3075 |
| redis | batch_jobs | 4 | 990.67 | 432.6368 | 444.5443 | 444.5443 | 444.5443 |
| redis | batch_jobs | 8 | 1218.39 | 253.0395 | 254.3987 | 254.3987 | 254.3987 |
| redis | batch_jobs | 16 | 1362.80 | 154.6173 | 156.7556 | 156.7556 | 156.7556 |
| redis | batch_jobs | 32 | 1494.03 | 95.4795 | 97.8511 | 97.8511 | 97.8511 |
| redis | mixed_priorities | 1 | 554.33 | 1196.0405 | 1332.8818 | 1332.8818 | 1332.8818 |
| redis | mixed_priorities | 2 | 742.44 | 774.9438 | 783.3982 | 783.3982 | 783.3982 |
| redis | mixed_priorities | 4 | 997.25 | 435.0985 | 435.9479 | 435.9479 | 435.9479 |
| redis | mixed_priorities | 8 | 1191.02 | 266.3673 | 269.8333 | 269.8333 | 269.8333 |
| redis | mixed_priorities | 16 | 1363.21 | 159.1973 | 166.8599 | 166.8599 | 166.8599 |
| redis | mixed_priorities | 32 | 1517.49 | 97.2459 | 98.4564 | 98.4564 | 98.4564 |
| redis | high_contention | 1 | 514.93 | 1169.4017 | 1203.6578 | 1203.6578 | 1203.6578 |
| redis | high_contention | 2 | 643.85 | 779.6440 | 787.9451 | 787.9451 | 787.9451 |
| redis | high_contention | 4 | 838.77 | 436.5845 | 439.1625 | 439.1625 | 439.1625 |
| redis | high_contention | 8 | 980.12 | 256.7513 | 271.9173 | 271.9173 | 271.9173 |
| redis | high_contention | 16 | 1088.22 | 152.6444 | 152.8445 | 152.8445 | 152.8445 |
| redis | high_contention | 32 | 1162.22 | 93.0375 | 94.5665 | 94.5665 | 94.5665 |
| redis | idle_queue | 1 | 0.00 | 0.1577 | 0.1758 | 0.1758 | 0.1758 |
| redis | idle_queue | 2 | 0.00 | 0.3073 | 0.3099 | 0.3099 | 0.3099 |
| redis | idle_queue | 4 | 0.00 | 0.6205 | 0.7271 | 0.7271 | 0.7271 |
| redis | idle_queue | 8 | 0.00 | 1.2795 | 1.2962 | 1.2962 | 1.2962 |
| redis | idle_queue | 16 | 0.00 | 2.4870 | 2.5917 | 2.5917 | 2.5917 |
| redis | idle_queue | 32 | 0.00 | 5.2835 | 5.5306 | 5.5306 | 5.5306 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
