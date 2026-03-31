"use client";

import { ReactNode } from "react";
import { ShellLayout } from "@/components/shell/shell-layout";
import { RoleGuard } from "@/components/auth/role-guard";

interface AdminLayoutProps {
  children: ReactNode;
}

export default function AdminLayout({ children }: AdminLayoutProps) {
  return (
    <RoleGuard capability="access_operator" fallbackHref="/app/dashboard">
      <ShellLayout
        activePath=""
        workspaceName="Admin Console"
        user={{
          email: "admin@example.com",
          name: "Admin"
        }}
      >
        {children}
      </ShellLayout>
    </RoleGuard>
  );
}
