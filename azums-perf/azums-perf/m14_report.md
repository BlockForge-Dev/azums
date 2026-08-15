# Azums M14 Performance Report

- generated_at_unix_ms: 1786773530509
- jobs_per_scenario: 1000
- iterations: 3
- batch_size: 64
- backends: memory, sqlite, postgres, redis
- profile: release

| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |
|---|---|---:|---:|---:|---:|---:|---:|
| memory | small_jobs | 1 | 103608.88 | 7.1246 | 8.3170 | 8.3170 | 8.3170 |
| memory | small_jobs | 2 | 102252.28 | 7.8237 | 8.4369 | 8.4369 | 8.4369 |
| memory | small_jobs | 4 | 95093.19 | 8.2730 | 9.5949 | 9.5949 | 9.5949 |
| memory | small_jobs | 8 | 100744.53 | 8.1196 | 8.1525 | 8.1525 | 8.1525 |
| memory | small_jobs | 16 | 96212.74 | 8.4192 | 8.4692 | 8.4692 | 8.4692 |
| memory | small_jobs | 32 | 91354.93 | 8.7711 | 8.8700 | 8.8700 | 8.8700 |
| memory | large_payloads | 1 | 33166.22 | 22.7051 | 30.0167 | 30.0167 | 30.0167 |
| memory | large_payloads | 2 | 32983.50 | 24.6880 | 31.8572 | 31.8572 | 31.8572 |
| memory | large_payloads | 4 | 26538.01 | 31.2990 | 40.7103 | 40.7103 | 40.7103 |
| memory | large_payloads | 8 | 29022.17 | 31.0069 | 31.8080 | 31.8080 | 31.8080 |
| memory | large_payloads | 16 | 29625.02 | 30.9148 | 30.9558 | 30.9558 | 30.9558 |
| memory | large_payloads | 32 | 28674.53 | 32.1454 | 32.3576 | 32.3576 | 32.3576 |
| memory | batch_jobs | 1 | 111129.59 | 6.9857 | 7.0619 | 7.0619 | 7.0619 |
| memory | batch_jobs | 2 | 102663.27 | 7.8644 | 8.0048 | 8.0048 | 8.0048 |
| memory | batch_jobs | 4 | 95321.09 | 8.3855 | 8.9252 | 8.9252 | 8.9252 |
| memory | batch_jobs | 8 | 96197.90 | 8.4812 | 8.5771 | 8.5771 | 8.5771 |
| memory | batch_jobs | 16 | 95427.75 | 8.5675 | 8.6027 | 8.6027 | 8.6027 |
| memory | batch_jobs | 32 | 91900.36 | 8.8715 | 8.9340 | 8.9340 | 8.9340 |
| memory | mixed_priorities | 1 | 112539.43 | 6.9783 | 6.9811 | 6.9811 | 6.9811 |
| memory | mixed_priorities | 2 | 101112.95 | 8.0669 | 8.1776 | 8.1776 | 8.1776 |
| memory | mixed_priorities | 4 | 96015.81 | 8.5074 | 8.5102 | 8.5102 | 8.5102 |
| memory | mixed_priorities | 8 | 99053.86 | 8.2210 | 8.3476 | 8.3476 | 8.3476 |
| memory | mixed_priorities | 16 | 96959.73 | 8.5040 | 8.5689 | 8.5689 | 8.5689 |
| memory | mixed_priorities | 32 | 94121.22 | 8.8550 | 8.8962 | 8.8962 | 8.8962 |
| memory | high_contention | 1 | 82646.25 | 7.5393 | 7.5455 | 7.5455 | 7.5455 |
| memory | high_contention | 2 | 76599.40 | 8.5199 | 8.5477 | 8.5477 | 8.5477 |
| memory | high_contention | 4 | 74131.47 | 8.8282 | 9.1875 | 9.1875 | 9.1875 |
| memory | high_contention | 8 | 73929.70 | 8.8836 | 9.0735 | 9.0735 | 9.0735 |
| memory | high_contention | 16 | 73353.02 | 9.0475 | 9.2358 | 9.2358 | 9.2358 |
| memory | high_contention | 32 | 71801.62 | 9.4245 | 9.4996 | 9.4996 | 9.4996 |
| memory | idle_queue | 1 | 0.00 | 0.0005 | 0.0018 | 0.0018 | 0.0018 |
| memory | idle_queue | 2 | 0.00 | 0.0006 | 0.0006 | 0.0006 | 0.0006 |
| memory | idle_queue | 4 | 0.00 | 0.0010 | 0.0011 | 0.0011 | 0.0011 |
| memory | idle_queue | 8 | 0.00 | 0.0014 | 0.0021 | 0.0021 | 0.0021 |
| memory | idle_queue | 16 | 0.00 | 0.0025 | 0.0041 | 0.0041 | 0.0041 |
| memory | idle_queue | 32 | 0.00 | 0.0049 | 0.0077 | 0.0077 | 0.0077 |
| sqlite | small_jobs | 1 | 1810.04 | 464.7458 | 469.0529 | 469.0529 | 469.0529 |
| sqlite | large_payloads | 1 | 1586.84 | 519.0295 | 519.6497 | 519.6497 | 519.6497 |
| sqlite | batch_jobs | 1 | 1748.45 | 472.7068 | 476.8510 | 476.8510 | 476.8510 |
| sqlite | mixed_priorities | 1 | 1717.21 | 484.5104 | 485.4502 | 485.4502 | 485.4502 |
| sqlite | high_contention | 1 | 1817.65 | 467.5578 | 483.6407 | 483.6407 | 483.6407 |
| sqlite | idle_queue | 1 | 0.00 | 0.2478 | 0.2661 | 0.2661 | 0.2661 |
| sqlite | idle_queue | 2 | 0.00 | 0.4641 | 0.5120 | 0.5120 | 0.5120 |
| sqlite | idle_queue | 4 | 0.00 | 0.8054 | 0.8892 | 0.8892 | 0.8892 |
| sqlite | idle_queue | 8 | 0.00 | 1.4501 | 1.5140 | 1.5140 | 1.5140 |
| sqlite | idle_queue | 16 | 0.00 | 2.5131 | 2.7363 | 2.7363 | 2.7363 |
| sqlite | idle_queue | 32 | 0.00 | 4.8759 | 5.1259 | 5.1259 | 5.1259 |
| postgres | small_jobs | 1 | 314.70 | 1413.3354 | 1536.4007 | 1536.4007 | 1536.4007 |
| postgres | small_jobs | 2 | 341.39 | 1165.4052 | 1266.0190 | 1266.0190 | 1266.0190 |
| postgres | small_jobs | 4 | 395.47 | 754.1862 | 757.5778 | 757.5778 | 757.5778 |
| postgres | small_jobs | 8 | 369.70 | 840.9056 | 1033.9876 | 1033.9876 | 1033.9876 |
| postgres | small_jobs | 16 | 353.13 | 997.7012 | 1017.7547 | 1017.7547 | 1017.7547 |
| postgres | small_jobs | 32 | 335.28 | 1107.9865 | 1186.0220 | 1186.0220 | 1186.0220 |
| postgres | large_payloads | 1 | 202.85 | 2887.2431 | 2932.1626 | 2932.1626 | 2932.1626 |
| postgres | large_payloads | 2 | 240.96 | 1909.2316 | 2131.9097 | 2131.9097 | 2131.9097 |
| postgres | large_payloads | 4 | 298.38 | 1146.6445 | 1178.7841 | 1178.7841 | 1178.7841 |
| postgres | large_payloads | 8 | 290.73 | 1215.3965 | 1371.8082 | 1371.8082 | 1371.8082 |
| postgres | large_payloads | 16 | 283.87 | 1341.3459 | 1350.8341 | 1350.8341 | 1350.8341 |
| postgres | large_payloads | 32 | 269.43 | 1493.0117 | 1525.1450 | 1525.1450 | 1525.1450 |
| postgres | batch_jobs | 1 | 197.99 | 3015.0049 | 3098.6793 | 3098.6793 | 3098.6793 |
| postgres | batch_jobs | 2 | 259.66 | 1814.5432 | 1934.0998 | 1934.0998 | 1934.0998 |
| postgres | batch_jobs | 4 | 296.22 | 1265.9305 | 1401.7274 | 1401.7274 | 1401.7274 |
| postgres | batch_jobs | 8 | 287.06 | 1373.0437 | 1377.1169 | 1377.1169 | 1377.1169 |
| postgres | batch_jobs | 16 | 276.47 | 1491.1090 | 1561.7553 | 1561.7553 | 1561.7553 |
| postgres | batch_jobs | 32 | 264.47 | 1666.3204 | 1698.0189 | 1698.0189 | 1698.0189 |
| postgres | mixed_priorities | 1 | 187.91 | 3157.4581 | 3376.2905 | 3376.2905 | 3376.2905 |
| postgres | mixed_priorities | 2 | 234.34 | 2053.0753 | 2152.8540 | 2152.8540 | 2152.8540 |
| postgres | mixed_priorities | 4 | 263.77 | 1571.1093 | 1583.4830 | 1583.4830 | 1583.4830 |
| postgres | mixed_priorities | 8 | 255.65 | 1652.5028 | 1715.5843 | 1715.5843 | 1715.5843 |
| postgres | mixed_priorities | 16 | 248.91 | 1708.2953 | 1882.1825 | 1882.1825 | 1882.1825 |
| postgres | mixed_priorities | 32 | 241.17 | 1769.1883 | 1797.6764 | 1797.6764 | 1797.6764 |
| postgres | high_contention | 1 | 171.87 | 3597.2528 | 3658.1836 | 3658.1836 | 3658.1836 |
| postgres | high_contention | 2 | 205.04 | 2503.2621 | 2589.3665 | 2589.3665 | 2589.3665 |
| postgres | high_contention | 4 | 238.16 | 1907.7615 | 1930.7768 | 1930.7768 | 1930.7768 |
| postgres | high_contention | 8 | 233.23 | 1874.4614 | 2120.2579 | 2120.2579 | 2120.2579 |
| postgres | high_contention | 16 | 231.74 | 1943.7604 | 2049.3750 | 2049.3750 | 2049.3750 |
| postgres | high_contention | 32 | 222.89 | 2051.4098 | 2079.3031 | 2079.3031 | 2079.3031 |
| postgres | idle_queue | 1 | 0.00 | 145.6166 | 157.0482 | 157.0482 | 157.0482 |
| postgres | idle_queue | 2 | 0.00 | 300.7369 | 306.6230 | 306.6230 | 306.6230 |
| postgres | idle_queue | 4 | 0.00 | 363.4350 | 367.4607 | 367.4607 | 367.4607 |
| postgres | idle_queue | 8 | 0.00 | 460.8947 | 469.9242 | 469.9242 | 469.9242 |
| postgres | idle_queue | 16 | 0.00 | 595.7210 | 600.1023 | 600.1023 | 600.1023 |
| postgres | idle_queue | 32 | 0.00 | 652.7966 | 662.4425 | 662.4425 | 662.4425 |
| redis | small_jobs | 1 | 903.33 | 725.0594 | 738.2752 | 738.2752 | 738.2752 |
| redis | small_jobs | 2 | 1186.62 | 462.1667 | 467.2313 | 467.2313 | 467.2313 |
| redis | small_jobs | 4 | 1620.26 | 251.8924 | 252.8573 | 252.8573 | 252.8573 |
| redis | small_jobs | 8 | 2045.37 | 144.0349 | 144.5238 | 144.5238 | 144.5238 |
| redis | small_jobs | 16 | 2158.19 | 92.9408 | 93.4208 | 93.4208 | 93.4208 |
| redis | small_jobs | 32 | 2440.87 | 63.8191 | 72.8664 | 72.8664 | 72.8664 |
| redis | large_payloads | 1 | 824.47 | 800.8505 | 824.8988 | 824.8988 | 824.8988 |
| redis | large_payloads | 2 | 1113.58 | 476.4875 | 494.4137 | 494.4137 | 494.4137 |
| redis | large_payloads | 4 | 1427.84 | 302.5464 | 308.1527 | 308.1527 | 308.1527 |
| redis | large_payloads | 8 | 1690.62 | 194.9086 | 205.2599 | 205.2599 | 205.2599 |
| redis | large_payloads | 16 | 1879.69 | 134.0623 | 135.2759 | 135.2759 | 135.2759 |
| redis | large_payloads | 32 | 1999.11 | 103.3892 | 106.4247 | 106.4247 | 106.4247 |
| redis | batch_jobs | 1 | 911.03 | 717.2113 | 743.7407 | 743.7407 | 743.7407 |
| redis | batch_jobs | 2 | 1198.47 | 452.4727 | 458.4891 | 458.4891 | 458.4891 |
| redis | batch_jobs | 4 | 1675.20 | 247.5543 | 251.3963 | 251.3963 | 251.3963 |
| redis | batch_jobs | 8 | 2068.15 | 141.0460 | 141.7376 | 141.7376 | 141.7376 |
| redis | batch_jobs | 16 | 2197.48 | 93.4071 | 93.5529 | 93.5529 | 93.5529 |
| redis | batch_jobs | 32 | 2382.08 | 63.1506 | 63.2998 | 63.2998 | 63.2998 |
| redis | mixed_priorities | 1 | 906.44 | 711.1525 | 737.9330 | 737.9330 | 737.9330 |
| redis | mixed_priorities | 2 | 1185.79 | 474.4404 | 480.2899 | 480.2899 | 480.2899 |
| redis | mixed_priorities | 4 | 1639.99 | 249.0151 | 255.2022 | 255.2022 | 255.2022 |
| redis | mixed_priorities | 8 | 2099.12 | 144.9002 | 148.9317 | 148.9317 | 148.9317 |
| redis | mixed_priorities | 16 | 2273.07 | 90.4608 | 90.7099 | 90.7099 | 90.7099 |
| redis | mixed_priorities | 32 | 2432.32 | 62.3028 | 62.6706 | 62.6706 | 62.6706 |
| redis | high_contention | 1 | 834.85 | 725.5963 | 731.3386 | 731.3386 | 731.3386 |
| redis | high_contention | 2 | 1062.81 | 457.5002 | 463.6935 | 463.6935 | 463.6935 |
| redis | high_contention | 4 | 1359.57 | 245.1113 | 250.7029 | 250.7029 | 250.7029 |
| redis | high_contention | 8 | 1603.81 | 141.6811 | 142.9372 | 142.9372 | 142.9372 |
| redis | high_contention | 16 | 1789.96 | 89.0127 | 92.4721 | 92.4721 | 92.4721 |
| redis | high_contention | 32 | 1881.41 | 61.4727 | 65.6139 | 65.6139 | 65.6139 |
| redis | idle_queue | 1 | 0.00 | 0.1176 | 0.1226 | 0.1226 | 0.1226 |
| redis | idle_queue | 2 | 0.00 | 0.2037 | 0.2078 | 0.2078 | 0.2078 |
| redis | idle_queue | 4 | 0.00 | 0.3696 | 0.4276 | 0.4276 | 0.4276 |
| redis | idle_queue | 8 | 0.00 | 0.8634 | 0.8694 | 0.8694 | 0.8694 |
| redis | idle_queue | 16 | 0.00 | 1.6147 | 1.6810 | 1.6810 | 1.6810 |
| redis | idle_queue | 32 | 0.00 | 3.2394 | 3.7763 | 3.7763 | 3.7763 |

## Conditions

- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.
- Percentiles are calculated across scenario iterations, not per-job handler latency.
- External backends are included only when their environment variables are configured.
- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.
