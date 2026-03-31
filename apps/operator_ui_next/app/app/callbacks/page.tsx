import { CallbacksPage } from "@/components/customer/callbacks-page";
import { RoleGuard } from "@/components/auth/role-guard";

export default function Page() {
  return (
    <RoleGuard capability="write_requests" title="Callbacks require developer/admin/owner role">
      <CallbacksPage />
    </RoleGuard>
  );
}
