<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { createMemoryClient, type EIP1193RequestFn, type JsonRpcResponse, type MemoryClient } from 'tevm';
	import { type PublicClient, custom, createPublicClient } from 'viem';
	import { whatsabi, loaders } from '@shazow/whatsabi';
	import { 
		PUBLIC_EXECUTION_RPC,
		PUBLIC_CONSENSUS_RPC 
	} from '$env/static/public';
	import { onMount } from 'svelte';

	let startMessage = $state<string>();
	let rpcUrl = $state<string>(PUBLIC_EXECUTION_RPC);
	const CONSENSUS_RPC = PUBLIC_CONSENSUS_RPC;

	let tevmClient = $state<MemoryClient | undefined>(undefined);
	let client = $state<PublicClient>(
		createPublicClient({
				transport: custom({
					request: (request) => {
						return invoke('request', {request:
							{
								...request,
								jsonrpc: '2.0',
								id: crypto.randomUUID(),
							}
						}).then((response: any) => {
							if (response.error) throw response.error
							return response.result
						}).catch(e => {
							throw e
						}) as any;
					}
				})
		})
	);

	let block = $state<any>();
	let contractAddress = $state<string>(localStorage.getItem('contractAddress') || '');
	let openAiKey = $state(localStorage.getItem('openai-key') || '');
	let etherscanKey = $state(localStorage.getItem('etherscan-key') || '');

	$effect(() => {
		if (openAiKey) {
			localStorage.setItem('openai-key', openAiKey);
		}
	});

	$effect(() => {
		if (etherscanKey) {
			localStorage.setItem('etherscan-key', etherscanKey);
		}
	});

	// Add a mounted flag
	let iframeMounted = $state(false);

	const start = async () => {
		startMessage = await invoke('start', {
			rpcUrl: rpcUrl,
			consensusRpc: CONSENSUS_RPC,
			chainId: 1
		}).catch(e => {
			return e as string
		}) as string;
	};

	const fork = () => {
		tevmClient = createMemoryClient({
			fork: {
				transport: client
			}
		});
	};
  const unfork = () => {
    tevmClient = undefined
  }

	const getBlock = () => {
		return client.getBlock({blockTag: 'latest'}).then(latestBlock => {
      block = latestBlock
    })
	};

	const handleAbi = async () => {
		try {
			const trimmedAddress = contractAddress.trim();
			
			// Create custom ABI loader with Etherscan if key is provided
			const config: any = { provider: client };
			if (etherscanKey.trim()) {
				config.abiLoader = new loaders.EtherscanABILoader({ 
					apiKey: etherscanKey.trim() 
				});
			}

			const result = await whatsabi.autoload(trimmedAddress, config);

			// Try to get contract name if available
			let contractName = 'Unknown Contract';
			try {
				const nameFunction = result.abi.find(item => 
					item.type === 'function' && 
					item.name === 'name' && 
					item.inputs?.length === 0
				);
				
				if (nameFunction) {
					const name = await client.readContract({
						address: trimmedAddress,
						abi: [nameFunction],
						functionName: 'name',
					});
					contractName = name as string;
				}
			} catch (error) {
				console.warn('Could not fetch contract name:', error);
			}

			const response = await fetch('https://api.openai.com/v1/chat/completions', {
				method: 'POST',
					headers: {
						'Content-Type': 'application/json',
						'Authorization': `Bearer ${openAiKey}`
					},
					body: JSON.stringify({
						model: "gpt-4-turbo-preview",
						messages: [{
							role: "system",
							content: "You are an expert frontend developer specializing in creating intuitive, well-structured HTML interfaces. Focus on clean, minimal styling while maintaining excellent usability. Group similar functionality together logically. Include proper error handling and loading states. Output only pure HTML code without any markdown formatting, code block tags, or explanations. The output will be directly injected into an HTML document."
						}, {
							role: "user",
							content: `Create a user interface for interacting with the "${contractName ?? trimmedAddress}" smart contract. Requirements:
							- Title should be "Generated contract interface for ${contractName}"
							- Output raw HTML without any markdown tags or code blocks (no \`\`\`html or similar)
							- Use only HTML and inline CSS (no external dependencies)
							- Group similar functionality together. For example, if multiple functions take a tokenId we should
							  be able to insert the tokenId once and view all relavent info in a table form
							- Writes should be similar. If we can transfer a tokenId we should have that functionality appear after
							  inserting a tokenId for example
							- Use your intuition. For example, if you notice the contract is a known contract like uniswap use your external knowledge of how uniswap works. Same for ERC20 ERC721 and any other type of contract
							- The user is currently logged in with address ${'0x9E2597dD51A8d4030AB7C2fBa66A061e9F709B20'}. Default to this value if it appears a specific piece of functionality applies to this value. If the address is unrelated to this address do not default.
							- When we default a specific value and that is the only value we need go ahead and eagerly fetch
							- Include minimal but professional styling
							- Add proper error handling and success messages
							- Make all inputs properly labeled and intuitive
							- Add loading states for async operations
							- Use the globally available 'viemProvider' object for blockchain interactions
							  (it's a Viem-compatible provider injected as window.viemProvider)
							- Contract address is: ${trimmedAddress}
							- Make sure all functionality of the contract abi is included
							- Think like a designer. Don't just put a series of inputs try to make an intuitive pleasant UI
							- Make use of modals when it makes sense
							- When building this UI first think about what the contract looks like and think of a product doc first
							- after making a product doc then create an initial versioon of the UI
							- after making the initial version review your UI to see if it's possible to polish and make better
							- finally do a final round of style and css polish
							
							Here's the ABI: ${JSON.stringify(result.abi)}`
						}],
						temperature: 0.7
					})
			});

			const data = await response.json();
			const generatedUI = data.choices?.[0]?.message?.content;
			if (!generatedUI) return;

			// Get iframe reference and inject content
			const iframe = document.querySelector('iframe');
			if (!iframe) return;

			const doc = iframe.contentWindow?.document;
			if (!doc) return;

			// Create and inject the provider before writing content
			const provider = {
				request: async (request: any) => {
					return invoke('request', {
						request: {
							...request,
							jsonrpc: '2.0',
							id: crypto.randomUUID(),
						}
					}).then((response: any) => {
						if (response.error) throw response.error;
						return response.result;
					});
				}
			};

			doc.open();
			doc.write(generatedUI);
			(iframe.contentWindow as any).viemProvider = provider;
			doc.close();

			console.log('UI and provider injected successfully');

		} catch (error) {
			console.error('Error:', error);
		}
	};

	$effect(() => {
		if (!startMessage) return;

		let timeoutId: NodeJS.Timeout;
		let isRunning = true
		const pollBlock = async () => {
				await getBlock();
				await handleAbi();
				if (!isRunning) return
				timeoutId = setTimeout(pollBlock, 10_000);
		};

		pollBlock();

		// Clean up effect
		return () => {
			isRunning = false
			clearTimeout(timeoutId);
		};
	});

	$effect(() => {
		localStorage.setItem('contractAddress', contractAddress);
	});

	$effect(() => {
		localStorage.setItem('openAiKey', openAiKey);
	});
</script>

<h1>Mana</h1>
<button onclick={start}>Start</button>
<button onclick={tevmClient ? unfork : fork}>{tevmClient ? 'Unfork' : 'Fork'}</button>
{#if startMessage}
	<p>{startMessage}</p>
{/if}
{#if block}
	<table>
		<thead>
			<tr>
				<th>Property</th>
				<th>Value</th>
			</tr>
		</thead>
		<tbody>
			<tr>
				<td>Hash</td>
				<td>{block.hash}</td>
			</tr>
			<tr>
				<td>Number</td>
				<td>{block.number}</td>
			</tr>
			<tr>
				<td>Parent Hash</td>
				<td>{block.parentHash}</td>
			</tr>
			<tr>
				<td>Timestamp</td>
				<td>{new Date(Number(block.timestamp) * 1000).toLocaleString()}</td>
			</tr>
			<tr>
				<td>Gas Used</td>
				<td>{block.gasUsed.toString()}</td>
			</tr>
			<tr>
				<td>Gas Limit</td>
				<td>{block.gasLimit.toString()}</td>
			</tr>
			<tr>
				<td>Base Fee</td>
				<td>{block.baseFeePerGas?.toString() ?? 'N/A'}</td>
			</tr>
			<tr>
				<td>Transactions</td>
				<td>{block.transactions.length} transactions</td>
			</tr>
		</tbody>
	</table>
{/if}

<div>
  <h2>Contract ABI Explorer</h2>
  <input 
    type="text" 
    bind:value={contractAddress} 
    placeholder="Enter contract address or ENS"
  />
  {#if contractAddress}
    <button onclick={handleAbi}>Get ABI</button>
  {/if}

  <iframe 
    sandbox="allow-scripts allow-same-origin"
    style="width: 100%; height: 800px; border: 2px solid red; margin-top: 20px;"
    title="Generated UI"
    srcdoc="<div style='text-align: center; padding: 20px; font-family: sans-serif;'>Contract UI will appear here</div>"
  ></iframe>
  <input 
    type="password"
    bind:value={openAiKey}
    placeholder="Enter OpenAI API Key"
  />

  <div class="input-group">
    <label for="etherscan-key">Etherscan API Key (optional):</label>
    <input 
      type="password" 
      id="etherscan-key"
      bind:value={etherscanKey}
      placeholder="Enter your Etherscan API key"
    />
  </div>

</div>

<p>Tauri <a href="https://v1.tauri.app/v1/guides/getting-started/setup/sveltekit">docs</a></p>
<p>Sveltekit <a href="https://svelte.dev/docs/kit">docs</a></p>
