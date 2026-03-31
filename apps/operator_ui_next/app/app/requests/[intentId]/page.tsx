import { RequestDetailPage } from "@/components/customer/request-detail-page";

export default async function Page({
  params,
}: Readonly<{
  params: Promise<{ intentId: string }>;
}>) {
  const resolved = await params;
  return <RequestDetailPage intentId={decodeURIComponent(resolved.intentId)} />;
}
