import { BillingPage } from "@/components/customer/billing-page";
import { RoleGuard } from "@/components/auth/role-guard";

export default function Page() {
  return (
    <RoleGuard capability="view_billing" title="Billing requires owner/admin role">
      <BillingPage />
    </RoleGuard>
  );
}
