import { ReceiptDetailPage } from "@/components/customer/receipt-detail-page";

export default async function Page({
  params,
}: Readonly<{
  params: Promise<{ receiptId: string }>;
}>) {
  const resolved = await params;
  return <ReceiptDetailPage receiptId={decodeURIComponent(resolved.receiptId)} />;
}
