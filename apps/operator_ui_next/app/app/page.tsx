"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { onboardingProgress, readSession } from "@/lib/app-state";

export default function Page() {
  const router = useRouter();

  useEffect(() => {
    let cancelled = false;
    void readSession().then((session) => {
      if (cancelled) return;
      if (!session) {
        router.replace("/login?next=%2Fapp%2Fdashboard");
        return;
      }
      const progress = onboardingProgress(session);
      if (progress.percent < 100) {
        router.replace("/app/onboarding");
      } else {
        router.replace("/app/dashboard");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [router]);

  return null;
}
