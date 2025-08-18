use std::io::Result;

fn main() -> Result<()> {
    // Only compile protos if the proto directory exists
    if std::path::Path::new("proto").exists() {
        println!("cargo:rerun-if-changed=proto");
        
        // Configure prost-build
        let mut config = prost_build::Config::new();
        config.protoc_arg("--experimental_allow_proto3_optional");
        
        // Compile the simplified protos without gogoproto dependencies
        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .compile_with_config(
                config,
                &[
                    "proto/cosmos/base/v1beta1/coin_simple.proto",
                    "proto/cosmwasm/wasm/v1/tx_simple.proto",
                    "proto/cosmwasm/wasm/v1/query.proto",
                    "proto/cosmos/tx/v1beta1/tx_simple.proto",
                    "proto/cosmos/tx/v1beta1/service.proto",
                    "proto/cosmos/auth/v1beta1/auth_simple.proto",
                    "proto/cosmos/auth/v1beta1/query.proto",
                    "proto/cosmos/bank/v1beta1/query.proto",
                    "proto/cosmos/base/tendermint/v1beta1/service.proto",
                    "proto/cosmos/vesting/v1beta1/vesting.proto",
                    "proto/injective/types/v1beta1/account.proto",
                    "proto/injective/crypto/v1beta1/ethsecp256k1/keys.proto",
                ],
                &["proto"],
            )?;
    }
    
    Ok(())
}