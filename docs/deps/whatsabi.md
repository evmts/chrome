# WhatsABI

TypeScript library for analyzing Ethereum bytecode, detecting proxies, and inferring ABIs.

## Core Usage

```typescript
import { whatsabi } from "@shazow/whatsabi";

// Basic
const result = await whatsabi.autoload(address, { provider });

// Manual
const code = await provider.getCode(address);
const selectors = whatsabi.selectorsFromBytecode(code);
const abi = whatsabi.abiFromBytecode(code);
```

## Configuration

```typescript
interface AutoloadConfig {
  provider: Provider;                   // Required
  followProxies?: boolean;             // Auto-resolve proxies
  loadContractResult?: boolean;        // Include metadata
  enableExperimentalMetadata?: boolean;// Include unreliable analysis
  abiLoader?: ABILoader | false;       // Custom loader
  signatureLookup?: SignatureLookup;   // Custom lookup
  onProgress?: (phase: string) => void;
  onError?: (phase: string, err: Error) => void;
}

interface AutoloadResult {
  abi: ABI;                           // Detected/loaded ABI
  address: string;                    // Contract address
  proxies: ProxyResolver[];          // Detected proxies
  abiLoadedFrom?: ABILoader;         // Source of ABI
  contractResult?: ContractResult;    // Full metadata
  isFactory?: boolean;               // Has CREATE/CREATE2
  followProxies?: (selector?: string) => Promise<AutoloadResult>;
}
```

## Provider Requirements

```typescript
interface Provider {
  getStorageAt(address: string, slot: number|string): Promise<string>;
  call(tx: { to: string, data: string }): Promise<string>;
  getCode(address: string): Promise<string>;
  getAddress(name: string): Promise<string>; // ENS
}
```
Supports: Ethers.js, Web3.js, Viem, EIP-1193, custom providers

## Proxy Support
Detects and resolves:
- EIP-1967 (Implementation & Beacon)
- Diamond (ERC-2535)
- ZeppelinOS
- UUPS (ERC-1822)
- Gnosis Safe
- Sequence Wallet
- Legacy Upgradeable
- EIP-1167 Minimal

## Signature Sources
- OpenChain (ex-Samczun)
- 4byte Directory
- Sourcify
- Etherscan
- Blockscout

## Error Types

```typescript
WhatsABIError
├── AutoloadError    // Autoload failures
├── LoaderError     // ABI/Contract loading
└── ProviderError   // Provider issues
```

## Performance
- Bytecode analysis: O(n) instructions
- RPC calls per proxy resolution
- External API calls for signatures
- Optional code caching via `WithCachedCode`
- Multiple reads for Diamond proxies

## Reliability
Strong:
- Function selector detection
- Basic proxy detection
- Storage slot analysis

Weak:
- Function modifiers (view/payable)
- Argument type detection
- Dynamic jump analysis
- Event parsing

## Advanced Usage

```typescript
// Custom loaders
const loader = new MultiABILoader([
  new SourcifyABILoader({ chainId }),
  new EtherscanABILoader({ apiKey })
]);

// Manual proxy handling
const result = await whatsabi.autoload(address, {
  provider,
  followProxies: false
});
if (result.proxies.length > 0) {
  const impl = await result.proxies[0].resolve(provider, address);
}
```
