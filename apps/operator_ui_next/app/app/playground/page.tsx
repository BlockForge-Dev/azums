import { PlaygroundPage } from "@/components/customer/playground-page";
import { RoleGuard } from "@/components/auth/role-guard";

export default function Page() {
  return (
    <RoleGuard
      capability="write_requests"
      title="Playground requires write access"
      description="Ask your workspace admin for developer/admin/owner role to run submissions."
    >
      <PlaygroundPage />
    </RoleGuard>
  );
}
