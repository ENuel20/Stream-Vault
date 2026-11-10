import * as StellarSdk from "@stellar/stellar-sdk";
import {
  isConnected,
  requestAccess,
  getAddress,
  signTransaction,
} from "@stellar/freighter-api";

export const TESTNET_RPC = "https://soroban-testnet.stellar.org";
export const PUBNET_RPC = "https://mainnet.sorobanrpc.com";
export const TESTNET_PASSPHRASE =
  StellarSdk.Networks.TESTNET as unknown as string;
export const PUBNET_PASSPHRASE =
  StellarSdk.Networks.PUBLIC as unknown as string;

export function getConfig() {
  const network =
    (process.env.NEXT_PUBLIC_STELLAR_NETWORK as string) || "testnet";
  const isPubnet = network === "pubnet";
  return {
    rpcUrl: isPubnet ? PUBNET_RPC : TESTNET_RPC,
    passphrase: isPubnet ? PUBNET_PASSPHRASE : TESTNET_PASSPHRASE,
    contractId: (process.env.NEXT_PUBLIC_VAULT_CONTRACT_ID as string) || "",
  };
}

export function server(): StellarSdk.rpc.Server {
  return new StellarSdk.rpc.Server(getConfig().rpcUrl, { allowHttp: false });
}

/** Connect Freighter and return the user's G... address. */
export async function connectWallet(): Promise<string> {
  const connected = await isConnected();
  if (!connected.isConnected) {
    throw new Error("Freighter wallet not detected. Install it to continue.");
  }
  const access = await requestAccess();
  if (access.error) throw new Error(access.error);
  const addr = await getAddress();
  if (addr.error) throw new Error(addr.error);
  return addr.address;
}

/** Load the source account for tx building. */
export async function loadAccount(
  publicKey: string
): Promise<StellarSdk.Account> {
  return server().getAccount(publicKey);
}

/**
 * Build, simulate, assemble, sign (Freighter), and submit a Soroban
 * contract invocation. Returns the send-transaction response hash.
 */
export async function invokeContract(params: {
  sourcePublicKey: string;
  contractId: string;
  method: string;
  args: StellarSdk.xdr.ScVal[];
}): Promise<{ hash: string; returnValue?: StellarSdk.xdr.ScVal }> {
  const { rpcUrl: _rpcUrl, passphrase } = getConfig();
  const srv = server();
  const account = await srv.getAccount(params.sourcePublicKey);
  const contract = new StellarSdk.Contract(params.contractId);

  const op = contract.call(params.method, ...params.args);
  let tx = new StellarSdk.TransactionBuilder(account, {
    fee: StellarSdk.BASE_FEE,
    networkPassphrase: passphrase,
  })
    .addOperation(op)
    .setTimeout(300)
    .build();

  // Simulate to fill footprint + auth.
  const sim = await srv.simulateTransaction(tx);
  if (StellarSdk.rpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${JSON.stringify(sim.error)}`);
  }
  const assembled = StellarSdk.rpc.assembleTransaction(
    tx,
    sim as StellarSdk.rpc.Api.SimulateTransactionSuccessResponse
  ).build();

  // Sign via Freighter.
  const signed = await signTransaction(assembled.toXDR(), {
    networkPassphrase: passphrase,
  });
  if (signed.error) throw new Error(signed.error);
  const signedTx = StellarSdk.TransactionBuilder.fromXDR(
    signed.signedTxXdr,
    passphrase
  ) as StellarSdk.Transaction;

  // Submit + poll.
  const send = await srv.sendTransaction(signedTx);
  if (send.status !== "PENDING") {
    throw new Error(`Submit failed: ${JSON.stringify(send)}`);
  }
  const deadline = Date.now() + 60_000;
  for (;;) {
    await new Promise((r) => setTimeout(r, 1500));
    const status = await srv.getTransaction(send.hash);
    if (status.status === "SUCCESS") {
      const meta = status as StellarSdk.rpc.Api.GetSuccessfulTransactionResponse;
      return { hash: send.hash, returnValue: meta.returnValue };
    }
    if (status.status === "FAILED") {
      throw new Error(`Transaction failed: ${send.hash}`);
    }
    if (Date.now() > deadline) throw new Error("Transaction timed out");
  }
}

// -- ScVal helpers -----------------------------------------------------

export const sc = {
  address: (v: string) =>
    new StellarSdk.Address(v).toScVal() as StellarSdk.xdr.ScVal,
  i128: (v: bigint | number | string) =>
    StellarSdk.nativeToScVal(BigInt(v), { type: "i128" }),
  u64: (v: bigint | number) =>
    StellarSdk.nativeToScVal(BigInt(v), { type: "u64" }),
  bool: (v: boolean) => StellarSdk.nativeToScVal(v, { type: "bool" }),
};
