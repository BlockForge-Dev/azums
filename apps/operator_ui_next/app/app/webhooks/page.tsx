import { RoleGuard } from "@/components/auth/role-guard";
import { WebhooksPage } from "@/components/customer/webhooks-page";

export default function Page() {
  return (
    <RoleGuard
      capability="manage_workspace"
      title="Webhook integration setup requires owner/admin role"
      description="Webhook source keys and callback boundaries are workspace-admin controls."
    >
      <WebhooksPage />
    </RoleGuard>
  );
}

