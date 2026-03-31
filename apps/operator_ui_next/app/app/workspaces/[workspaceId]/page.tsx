import { WorkspaceDetailPage } from "@/components/customer/workspace-detail-page";

export default async function Page({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const resolved = await params;
  return <WorkspaceDetailPage workspaceId={decodeURIComponent(resolved.workspaceId)} />;
}
