import { RoleGuard } from "@/components/auth/role-guard";
import { CallbackDetailPage } from "@/components/customer/callback-detail-page";

export default async function Page({
  params,
}: Readonly<{
  params: Promise<{ callbackId: string }>;
}>) {
  const resolved = await params;
  return (
    <RoleGuard capability="write_requests" title="Callback detail requires developer/admin/owner role">
      <CallbackDetailPage callbackId={decodeURIComponent(resolved.callbackId)} />
    </RoleGuard>
  );
}
