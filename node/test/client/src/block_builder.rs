// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{Client, FullBackend};
use parity_scale_codec::{Decode, Encode};
use polkadot_primitives::{Block, InherentData as ParachainsInherentData};
use polkadot_test_runtime::{GetLastTimestamp, UncheckedExtrinsic};
use sc_block_builder::{BlockBuilder, BlockBuilderProvider};
use sp_api::ProvideRuntimeApi;
use sp_consensus_babe::{
	digests::{PreDigest, SecondaryPlainPreDigest},
	BABE_ENGINE_ID,
};
use sp_runtime::{generic::BlockId, traits::Block as BlockT, Digest, DigestItem};
use sp_state_machine::BasicExternalities;

/// An extension for the test client to initialize a Polkadot specific block builder.
pub trait InitPolkadotBlockBuilder {
	/// Init a Polkadot specific block builder that works for the test runtime.
	///
	/// This will automatically create and push the inherents for you to make the block valid for the test runtime.
	fn init_polkadot_block_builder(
		&self,
	) -> sc_block_builder::BlockBuilder<Block, Client, FullBackend>;

	/// Init a Polkadot specific block builder at a specific block that works for the test runtime.
	///
	/// Same as [`InitPolkadotBlockBuilder::init_polkadot_block_builder`] besides that it takes a [`BlockId`] to say
	/// which should be the parent block of the block that is being build.
	fn init_polkadot_block_builder_at(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> sc_block_builder::BlockBuilder<Block, Client, FullBackend>;
}

impl InitPolkadotBlockBuilder for Client {
	fn init_polkadot_block_builder(&self) -> BlockBuilder<Block, Client, FullBackend> {
		let chain_info = self.chain_info();
		self.init_polkadot_block_builder_at(chain_info.best_hash)
	}

	fn init_polkadot_block_builder_at(
		&self,
		hash: <Block as BlockT>::Hash,
	) -> BlockBuilder<Block, Client, FullBackend> {
		let last_timestamp =
			self.runtime_api().get_last_timestamp(hash).expect("Get last timestamp");

		// `MinimumPeriod` is a storage parameter type that requires externalities to access the value.
		let minimum_period = BasicExternalities::new_empty()
			.execute_with(|| polkadot_test_runtime::MinimumPeriod::get());

		let timestamp = if last_timestamp == 0 {
			std::time::SystemTime::now()
				.duration_since(std::time::SystemTime::UNIX_EPOCH)
				.expect("Time is always after UNIX_EPOCH; qed")
				.as_millis() as u64
		} else {
			last_timestamp + minimum_period
		};

		// `SlotDuration` is a storage parameter type that requires externalities to access the value.
		let slot_duration = BasicExternalities::new_empty()
			.execute_with(|| polkadot_test_runtime::SlotDuration::get());

		let slot = (timestamp / slot_duration).into();

		let digest = Digest {
			logs: vec![DigestItem::PreRuntime(
				BABE_ENGINE_ID,
				PreDigest::SecondaryPlain(SecondaryPlainPreDigest { slot, authority_index: 42 })
					.encode(),
			)],
		};

		let mut block_builder = self
			.new_block_at(&BlockId::Hash(hash), digest, false)
			.expect("Creates new block builder for test runtime");

		let mut inherent_data = sp_inherents::InherentData::new();

		inherent_data
			.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp)
			.expect("Put timestamp inherent data");

		let parent_header = self
			.header(hash)
			.expect("Get the parent block header")
			.expect("The target block header must exist");

		let parachains_inherent_data = ParachainsInherentData {
			bitfields: Vec::new(),
			backed_candidates: Vec::new(),
			disputes: Vec::new(),
			parent_header,
		};

		inherent_data
			.put_data(
				polkadot_primitives::PARACHAINS_INHERENT_IDENTIFIER,
				&parachains_inherent_data,
			)
			.expect("Put parachains inherent data");

		let inherents = block_builder.create_inherents(inherent_data).expect("Creates inherents");

		inherents
			.into_iter()
			.for_each(|ext| block_builder.push(ext).expect("Pushes inherent"));

		block_builder
	}
}

/// Polkadot specific extensions for the [`BlockBuilder`].
pub trait BlockBuilderExt {
	/// Push a Polkadot test runtime specific extrinsic to the block.
	///
	/// This will internally use the [`BlockBuilder::push`] method, but this method expects a opaque extrinsic. So,
	/// we provide this wrapper which converts a test runtime specific extrinsic to a opaque extrinsic and pushes it to
	/// the block.
	///
	/// Returns the result of the application of the extrinsic.
	fn push_polkadot_extrinsic(
		&mut self,
		ext: UncheckedExtrinsic,
	) -> Result<(), sp_blockchain::Error>;
}

impl BlockBuilderExt for BlockBuilder<'_, Block, Client, FullBackend> {
	fn push_polkadot_extrinsic(
		&mut self,
		ext: UncheckedExtrinsic,
	) -> Result<(), sp_blockchain::Error> {
		let encoded = ext.encode();
		self.push(
			Decode::decode(&mut &encoded[..]).expect(
				"The runtime specific extrinsic always decodes to an opaque extrinsic; qed",
			),
		)
	}
}
