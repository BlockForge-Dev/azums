import { ApiKeysPage } from "@/components/customer/api-keys-page";
import { RoleGuard } from "@/components/auth/role-guard";

export default function Page() {
  return (
    <RoleGuard capability="write_requests" title="API key management requires write access">
      <ApiKeysPage />
    </RoleGuard>
  );
}
