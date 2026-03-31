"use client";

import { ReactNode } from "react";
import { ShellLayout } from "@/components/shell/shell-layout";
import { RoleGuard } from "@/components/auth/role-guard";

interface OpsLayoutProps {
  children: ReactNode;
}

export default function OpsLayout({ children }: OpsLayoutProps) {
  return (
    <RoleGuard capability="access_operator" fallbackHref="/app/dashboard">
      <ShellLayout
        activePath=""
        workspaceName="Operator Console"
        user={{
          email: "operator@example.com",
          name: "Operator"
        }}
      >
        {children}
      </ShellLayout>
    </RoleGuard>
  );
}
