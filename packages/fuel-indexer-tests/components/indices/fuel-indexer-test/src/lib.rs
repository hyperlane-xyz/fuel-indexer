extern crate alloc;
use fuel_indexer_macros::indexer;
use fuel_indexer_plugin::prelude::*;

// Copied over from the block-explorer example
pub fn derive_id(id: [u8; 32], data: Vec<u8>) -> Bytes32 {
    let mut buff: [u8; 32] = [0u8; 32];
    let result = [id.to_vec(), data].concat();
    buff.copy_from_slice(&sha256_digest(&result).as_bytes()[..32]);
    Bytes32::from(buff)
}

#[indexer(manifest = "packages/fuel-indexer-tests/assets/fuel_indexer_test.yaml")]
mod fuel_indexer_test {

    fn fuel_indexer_test_ping(ping: Ping) {
        Logger::info("fuel_indexer_test_ping handling a Ping event.");

        let entity = PingEntity {
            id: ping.id,
            value: ping.value,
            message: ping.message.to_string(),
        };

        entity.save();
    }

    fn fuel_indexer_test_blocks(block_data: BlockData) {
        let block = Block {
            id: block_data.id,
            height: block_data.height,
            timestamp: block_data.time,
        };

        block.save();

        let input_data = r#"{"foo":"bar"}"#.to_string();

        for tx in block_data.transactions.iter() {
            let tx = Tx {
                id: tx.id,
                block: block.id,
                timestamp: block_data.time,
                input_data: Json(input_data.clone()),
            };
            tx.save();
        }
    }

    fn fuel_indexer_test_transfer(transfer: abi::Transfer) {
        Logger::info("fuel_indexer_test_transfer handling Transfer event.");

        let abi::Transfer {
            contract_id,
            to,
            asset_id,
            amount,
            ..
        } = transfer;

        let entity = Transfer {
            id: derive_id(
                *contract_id,
                [contract_id.to_vec(), to.to_vec(), asset_id.to_vec()].concat(),
            ),
            contract_id,
            recipient: to,
            amount,
            asset_id,
        };

        entity.save();
    }

    fn fuel_indexer_test_transferout(transferout: abi::TransferOut) {
        Logger::info("fuel_indexer_test_transferout handling TransferOut event.");

        let abi::TransferOut {
            contract_id,
            to,
            asset_id,
            amount,
            ..
        } = transferout;

        let entity = TransferOut {
            id: derive_id(
                *contract_id,
                [contract_id.to_vec(), to.to_vec(), asset_id.to_vec()].concat(),
            ),
            contract_id,
            recipient: to,
            amount,
            asset_id,
        };

        entity.save();
    }

    fn fuel_indexer_test_log(log: abi::Log) {
        Logger::info("fuel_indexer_test_log handling Log event.");

        let abi::Log {
            contract_id, rb, ..
        } = log;

        let entity = Log {
            id: derive_id(*contract_id, u64::to_le_bytes(rb).to_vec()),
            contract_id: log.contract_id,
            ra: log.ra,
            rb: log.rb,
        };

        entity.save();
    }

    fn fuel_indexer_test_logdata(logdata_entity: Pung) {
        Logger::info("fuel_indexer_test_logdata handling LogData event.");

        let entity = PungEntity {
            id: logdata_entity.id,
            value: logdata_entity.value,
            is_pung: logdata_entity.is_pung,
            // TODO: https://github.com/FuelLabs/fuel-indexer/issues/386
            pung_from: Identity::from(logdata_entity.pung_from),
        };

        entity.save();
    }

    fn fuel_indexer_test_scriptresult(scriptresult: abi::ScriptResult) {
        Logger::info("fuel_indexer_test_scriptresult handling ScriptResult event.");

        let abi::ScriptResult { result, gas_used } = scriptresult;

        let entity = ScriptResult {
            id: derive_id([0u8; 32], u64::to_be_bytes(result).to_vec()),
            result,
            gas_used,
        };

        entity.save();
    }

    fn fuel_indexer_test_messageout(messageout: abi::MessageOut) {
        Logger::info("fuel_indexer_test_messageout handling MessageOut event");

        let abi::MessageOut {
            sender,
            message_id,
            recipient,
            amount,
            nonce,
            len,
            digest,
            ..
        } = messageout;

        let entity = MessageOut {
            id: message_id,
            sender,
            recipient,
            amount,
            nonce,
            len,
            digest,
        };

        entity.save();
    }

    fn fuel_indexer_test_callreturn(pungentity: Pung) {
        Logger::info("fuel_indexer_test_callreturn handling Pung event.");

        let entity = PungEntity {
            id: pungentity.id,
            value: pungentity.value,
            is_pung: pungentity.is_pung,
            // TODO: https://github.com/FuelLabs/fuel-indexer/issues/386
            pung_from: Identity::from(pungentity.pung_from),
        };

        entity.save();
    }

    fn fuel_indexer_test_multiargs(
        pung: Pung,
        pong: Pong,
        ping: Ping,
        block_data: BlockData,
    ) {
        Logger::info("fuel_indexer_test_multiargs handling Pung, Pong, Ping, and BlockData events.");

        let block = Block {
            id: block_data.id,
            height: block_data.height,
            timestamp: block_data.time,
        };

        block.save();

        let pu = PungEntity {
            id: pung.id,
            value: pung.value,
            is_pung: pung.is_pung,
            pung_from: Identity::from(pung.pung_from),
        };

        pu.save();

        let po = PongEntity {
            id: pong.id,
            value: pong.value,
        };

        po.save();

        let pi = PingEntity {
            id: ping.id,
            value: ping.value,
            message: ping.message.to_string(),
        };

        pi.save();
    }
}
