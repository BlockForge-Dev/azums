import {
  ComputeBudgetProgram,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import bs58 from "bs58";

const payerSecretRaw = process.env.PAYER_SECRET_BASE58?.trim();
const latestBlockhash = process.env.LATEST_BLOCKHASH?.trim();
const toPubkeyEnv = process.env.TO_PUBKEY?.trim();
const lamportsRaw = process.env.LAMPORTS?.trim();
const cuLimitRaw = process.env.CU_LIMIT?.trim();
const cuPriceRaw = process.env.CU_PRICE_MICROLAMPORTS?.trim();

if (!payerSecretRaw) {
  throw new Error("Missing PAYER_SECRET_BASE58");
}
if (!latestBlockhash) {
  throw new Error("Missing LATEST_BLOCKHASH");
}

const lamports = lamportsRaw ? Number.parseInt(lamportsRaw, 10) : 5000;
if (!Number.isInteger(lamports) || lamports <= 0) {
  throw new Error("LAMPORTS must be a positive integer");
}

const cuLimit = cuLimitRaw ? Number.parseInt(cuLimitRaw, 10) : null;
if (cuLimitRaw && (!Number.isInteger(cuLimit) || cuLimit <= 0)) {
  throw new Error("CU_LIMIT must be a positive integer");
}

const cuPriceMicroLamports = cuPriceRaw ? Number.parseInt(cuPriceRaw, 10) : null;
if (
  cuPriceRaw &&
  (!Number.isInteger(cuPriceMicroLamports) || cuPriceMicroLamports < 0)
) {
  throw new Error("CU_PRICE_MICROLAMPORTS must be a non-negative integer");
}

let payerSecretBytes;
if (payerSecretRaw.startsWith("[")) {
  const arr = JSON.parse(payerSecretRaw);
  if (!Array.isArray(arr)) {
    throw new Error("PAYER_SECRET_BASE58 JSON must be an array of bytes");
  }
  payerSecretBytes = Uint8Array.from(arr);
} else {
  payerSecretBytes = bs58.decode(payerSecretRaw);
}

const payer = Keypair.fromSecretKey(payerSecretBytes);
const receiver = toPubkeyEnv ? new PublicKey(toPubkeyEnv) : Keypair.generate().publicKey;

const tx = new Transaction({
  feePayer: payer.publicKey,
  recentBlockhash: latestBlockhash,
});

if (cuLimit !== null) {
  tx.add(
    ComputeBudgetProgram.setComputeUnitLimit({
      units: cuLimit,
    }),
  );
}
if (cuPriceMicroLamports !== null) {
  tx.add(
    ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: cuPriceMicroLamports,
    }),
  );
}

tx.add(
  SystemProgram.transfer({
    fromPubkey: payer.publicKey,
    toPubkey: receiver,
    lamports,
  }),
);

tx.sign(payer);

const sigBytes = tx.signatures[0].signature;
if (!sigBytes) {
  throw new Error("Missing signature after signing");
}

const signedTxBase64 = Buffer.from(tx.serialize()).toString("base64");
const expectedSignature = bs58.encode(sigBytes);

console.log(
  JSON.stringify(
    {
      from_pubkey: payer.publicKey.toBase58(),
      to_pubkey: receiver.toBase58(),
      lamports,
      cu_limit: cuLimit,
      cu_price_micro_lamports: cuPriceMicroLamports,
      recent_blockhash: latestBlockhash,
      expected_signature: expectedSignature,
      signed_tx_base64: signedTxBase64,
    },
    null,
    2,
  ),
);
