import express from 'express';
import bodyParser from 'body-parser';
import dotenv from 'dotenv';
import { 
  MsgExecuteContract,
  MsgExecuteContractCompat,
  ChainRestAuthApi,
  createTransaction,
  createTransactionWithSigners,
  createTxRawEIP712,
  createWeb3Extension,
  SIGN_EIP712,
  PrivateKey,
  TxRestApi,
  BaseAccount,
  getEip712TypedData,
} from '@injectivelabs/sdk-ts';
import { 
  Network,
  getNetworkEndpoints,
} from '@injectivelabs/networks';
import { EthereumChainId } from '@injectivelabs/ts-types';
import { BigNumberInBase } from '@injectivelabs/utils';

dotenv.config();

interface SignRequest {
  chain_id: string;
  account_number: number;
  sequence: number;
  messages: MessageData[];
  gas_limit: number;
  gas_price: string;
  memo: string;
  request_id: string;
}

interface MessageData {
  contract: string;
  msg: any;
  funds: Coin[];
}

interface Coin {
  denom: string;
  amount: string;
}

interface SignResponse {
  success: boolean;
  tx_hash?: string;
  error?: string;
  request_id: string;
}

class BridgeService {
  private privateKey: PrivateKey;
  private network: Network;
  private endpoints: any;
  private chainId: string;
  private address: string;
  private publicKey: string;
  private ethereumChainId: EthereumChainId;

  constructor(mnemonic: string, networkName: string) {
    // Initialize private key from mnemonic
    this.privateKey = PrivateKey.fromMnemonic(mnemonic);
    this.publicKey = this.privateKey.toPublicKey().toBase64();
    
    // Get injective address
    const injectiveAddress = this.privateKey.toAddress();
    this.address = injectiveAddress.address;
    console.log(`Bridge initialized for address: ${this.address}`);
    
    // Setup network
    this.network = networkName === 'mainnet' ? Network.Mainnet : Network.Testnet;
    this.endpoints = getNetworkEndpoints(this.network);
    this.chainId = networkName === 'mainnet' ? 'injective-1' : 'injective-888';
    // Use the correct Ethereum chain ID for Injective testnet (1439 instead of Goerli's 5)
    this.ethereumChainId = networkName === 'mainnet' ? EthereumChainId.Mainnet : 1439;
    
    console.log(`Using network: ${networkName} (${this.chainId})`);
    console.log(`RPC endpoint: ${this.endpoints.rest}`);
  }

  /**
   * Transform empty objects to have dummy fields to work around SDK limitation
   * The SDK cannot generate EIP-712 types for empty objects.
   * However, we need to be careful not to modify contract message structures.
   */
  private transformEmptyObjects(obj: any, depth: number = 0): any {
    if (obj === null || obj === undefined) {
      return obj;
    }
    
    if (typeof obj !== 'object') {
      return obj;
    }
    
    if (Array.isArray(obj)) {
      return obj.map(item => this.transformEmptyObjects(item, depth + 1));
    }
    
    // Clone the object
    const result: any = {};
    const keys = Object.keys(obj);
    
    // Only add dummy fields to top-level empty objects, not nested ones
    // This prevents contract message corruption
    if (keys.length === 0 && depth === 0) {
      console.log('[Bridge] Adding dummy field to top-level empty object');
      result._dummy = true;
      return result;
    }
    
    // For nested empty objects (like {"advance_epoch": {}}), keep them as is
    if (keys.length === 0) {
      console.log('[Bridge] Preserving nested empty object to avoid contract schema issues');
      return {};
    }
    
    // Recursively process all properties
    for (const key of keys) {
      result[key] = this.transformEmptyObjects(obj[key], depth + 1);
    }
    
    return result;
  }

  async handleSignAndBroadcast(req: SignRequest): Promise<SignResponse> {
    try {
      console.log(`[${req.request_id}] Processing sign request`);
      console.log(`[${req.request_id}] Contract: ${req.messages[0]?.contract}`);
      console.log(`[${req.request_id}] Message: ${JSON.stringify(req.messages[0]?.msg)}`);
      
      // Create messages array
      const messages = req.messages.map(msgData => {
        // Fix for SDK empty object issue
        let processedMsg = msgData.msg;
        
        // Transform empty objects to have a dummy field
        processedMsg = this.transformEmptyObjects(processedMsg);
        console.log(`[${req.request_id}] Transformed message:`, JSON.stringify(processedMsg));
        
        // Create the message with funds handling
        // The SDK expects either 'msg' or 'exec' parameter, and the msg should be an object
        const msgJSON: any = {
          sender: this.address,
          contractAddress: msgData.contract,
          msg: processedMsg,  // This needs to be an object, not a string
        };
        
        // Only add funds if non-empty to avoid SDK issue
        if (msgData.funds && msgData.funds.length > 0) {
          msgJSON.funds = msgData.funds.map(coin => ({
            denom: coin.denom,
            amount: coin.amount,
          }));
        } else {
          // Explicitly set funds to undefined to avoid empty array issue
          msgJSON.funds = undefined;
        }
        
        console.log(`[${req.request_id}] Creating MsgExecuteContractCompat with:`, JSON.stringify(msgJSON));
        // Use MsgExecuteContractCompat which is designed for Injective
        const executeMsg = MsgExecuteContractCompat.fromJSON(msgJSON);
        
        // Debug: Check what the message looks like after creation
        console.log(`[${req.request_id}] Created message type:`, executeMsg.constructor.name);
        console.log(`[${req.request_id}] Message has funds field:`, 'funds' in executeMsg);
        
        // Workaround: Try to delete empty funds array if it exists
        const msgAny = executeMsg as any;
        if (msgAny.funds && Array.isArray(msgAny.funds) && msgAny.funds.length === 0) {
          console.log(`[${req.request_id}] WARNING: Empty funds array detected, attempting to delete`);
          delete msgAny.funds;
        }
        
        return executeMsg;
      });

      // Get account details if not provided
      let accountNumber = req.account_number;
      let sequence = req.sequence;
      
      if (!accountNumber) {
        console.log(`[${req.request_id}] Fetching account details...`);
        const accountApi = new ChainRestAuthApi(this.endpoints.rest);
        const accountResponse = await accountApi.fetchAccount(this.address);
        const baseAccount = BaseAccount.fromRestApi(accountResponse);
        accountNumber = parseInt(baseAccount.accountNumber.toString());
        sequence = parseInt(baseAccount.sequence.toString());
        console.log(`[${req.request_id}] Account #${accountNumber}, Sequence: ${sequence}`);
      }

      // Create fee
      const fee = {
        amount: [{
          denom: 'inj',
          amount: new BigNumberInBase(0.0005).toWei().toFixed(), // 0.0005 INJ
        }],
        gas: req.gas_limit.toString(),
      };

      // Debug: Log messages before EIP-712 conversion
      console.log(`[${req.request_id}] Messages before EIP-712:`);
      messages.forEach((msg, idx) => {
        console.log(`  Message ${idx}:`, JSON.stringify(msg.toDirectSign ? msg.toDirectSign() : msg));
      });
      
      // Create EIP-712 typed data
      console.log(`[${req.request_id}] Creating EIP-712 typed data...`);
      let eip712TypedData;
      try {
        eip712TypedData = getEip712TypedData({
          msgs: messages,
          tx: {
            accountNumber: accountNumber.toString(),
            sequence: sequence.toString(),
            chainId: this.chainId,
            memo: req.memo || '',
            timeoutHeight: '0',
          },
          fee: fee,
          ethereumChainId: this.ethereumChainId,
        });
        
        // Debug log the exact typed data structure
        console.error('DEBUG: EIP-712 typed data structure:');
        console.error(JSON.stringify(eip712TypedData, null, 2));
      } catch (eipError: any) {
        console.error(`[${req.request_id}] EIP-712 creation failed:`, eipError);
        console.error(`[${req.request_id}] Error stack:`, eipError.stack);
        
        // If it's the empty array error, try alternative approach
        if (eipError.message && eipError.message.includes('Array with length 0')) {
          console.log(`[${req.request_id}] Attempting workaround for empty array issue...`);
          
          // Create a custom message that bypasses the issue
          const customMessages = messages.map(msg => {
            const msgObj = msg.toDirectSign ? msg.toDirectSign() : msg;
            // Remove any empty arrays from the message
            const cleanMsg = JSON.parse(JSON.stringify(msgObj, (key, value) => {
              if (Array.isArray(value) && value.length === 0) {
                return undefined; // Remove empty arrays
              }
              return value;
            }));
            return MsgExecuteContractCompat.fromJSON(cleanMsg);
          });
          
          // Retry with cleaned messages
          eip712TypedData = getEip712TypedData({
            msgs: customMessages,
            tx: {
              accountNumber: accountNumber.toString(),
              sequence: sequence.toString(),
              chainId: this.chainId,
              memo: req.memo || '',
              timeoutHeight: '0',
            },
            fee: fee,
            ethereumChainId: this.ethereumChainId,
          });
          console.log(`[${req.request_id}] Workaround successful!`);
        } else {
          throw eipError;
        }
      }

      console.log(`[${req.request_id}] Signing EIP-712 transaction...`);
      
      // Sign the EIP-712 typed data
      const signature = await this.privateKey.signTypedData(eip712TypedData);

      // Create the transaction with EIP-712 sign mode
      const { txRaw } = createTransactionWithSigners({
        chainId: this.chainId,
        fee: fee,
        message: messages,
        memo: req.memo || '',
        signers: {
          pubKey: this.publicKey,
          accountNumber: accountNumber,
          sequence: sequence,
        },
        signMode: SIGN_EIP712,  // Use EIP-712 sign mode
      });

      // Create Web3 extension for EIP-712
      const web3Extension = createWeb3Extension({
        ethereumChainId: this.ethereumChainId,
      });

      // Create the EIP-712 transaction with the extension
      const txRawEIP712 = createTxRawEIP712(txRaw, web3Extension);

      // Set the EIP-712 signature
      txRawEIP712.signatures = [signature];

      // Broadcast the EIP-712 transaction
      console.log(`[${req.request_id}] Broadcasting EIP-712 transaction...`);
      const txApi = new TxRestApi(this.endpoints.rest);
      const broadcastResponse = await txApi.broadcast(txRawEIP712);

      // Check response
      if (broadcastResponse.code && broadcastResponse.code !== 0) {
        throw new Error(`Transaction failed with code ${broadcastResponse.code}: ${broadcastResponse.rawLog}`);
      }

      const txHash = broadcastResponse.txHash;
      console.log(`[${req.request_id}] Transaction successful: ${txHash}`);

      return {
        success: true,
        tx_hash: txHash,
        request_id: req.request_id,
      };
    } catch (error: any) {
      console.error(`[${req.request_id}] Error:`, error);
      return {
        success: false,
        error: error.message || 'Unknown error',
        request_id: req.request_id,
      };
    }
  }

  async health(): Promise<any> {
    return {
      status: 'healthy',
      service: 'gmine-bridge-nodejs',
      version: '1.0.0',
      network: this.network === Network.Testnet ? 'testnet' : 'mainnet',
      address: this.address,
      chainId: this.chainId,
    };
  }
}

// Main application
async function main() {
  // Load environment
  const mnemonic = process.env.MNEMONIC;
  if (!mnemonic) {
    console.error('MNEMONIC environment variable is required');
    process.exit(1);
  }

  const network = process.env.NETWORK || 'testnet';
  const port = process.env.PORT || '8080';
  const apiKey = process.env.BRIDGE_API_KEY;

  // Create bridge service
  const bridge = new BridgeService(mnemonic, network);

  // Create Express app
  const app = express();
  app.use(bodyParser.json());

  // Logging middleware
  app.use((req, res, next) => {
    console.log(`${new Date().toISOString()} ${req.method} ${req.path}`);
    next();
  });

  // Middleware for API key validation
  const validateApiKey = (req: any, res: any, next: any) => {
    if (apiKey) {
      const providedKey = req.headers['x-api-key'];
      if (providedKey !== apiKey) {
        return res.status(401).json({ error: 'Unauthorized' });
      }
    }
    next();
  };

  // Health endpoint
  app.get('/health', async (req, res) => {
    const health = await bridge.health();
    res.json(health);
  });

  // Sign and broadcast endpoint
  app.post('/sign-and-broadcast', validateApiKey, async (req, res) => {
    const response = await bridge.handleSignAndBroadcast(req.body);
    if (!response.success) {
      res.status(400);
    }
    res.json(response);
  });

  // Start server (bind to localhost only for security)
  app.listen(parseInt(port), '127.0.0.1', () => {
    console.log(`Bridge service (Node.js) listening on 127.0.0.1:${port}`);
    console.log(`Network: ${network}`);
    console.log(`Chain ID: ${bridge['chainId']}`);
    console.log(`Address: ${bridge['address']}`);
  });
}

// Run the service
main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});