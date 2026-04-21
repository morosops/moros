pragma circom 2.1.9;

include "../../node_modules/circomlib/circuits/poseidon.circom";
include "../../node_modules/circomlib/circuits/bitify.circom";
include "../../node_modules/circomlib/circuits/comparators.circom";

template CardLeaf() {
    signal input cardId;
    signal input salt;
    signal output out;

    component hash = Poseidon(2);
    hash.inputs[0] <== cardId;
    hash.inputs[1] <== salt;
    out <== hash.out;
}

template CardRankFromId() {
    signal input cardId;
    signal output rank;

    signal q;
    signal r;

    q <-- cardId \ 13;
    r <-- cardId % 13;

    component cardBound = LessThan(9);
    cardBound.in[0] <== cardId;
    cardBound.in[1] <== 312;
    cardBound.out === 1;

    component qBound = LessThan(5);
    qBound.in[0] <== q;
    qBound.in[1] <== 24;
    qBound.out === 1;

    component rBound = LessThan(4);
    rBound.in[0] <== r;
    rBound.in[1] <== 13;
    rBound.out === 1;

    cardId === q * 13 + r;
    rank <== r + 1;
}

template PoseidonMerkleRoot(depth) {
    signal input leaf;
    signal input index;
    signal input siblings[depth];
    signal output root;

    component bits = Num2Bits(depth);
    bits.in <== index;

    signal levelHashes[depth + 1];
    signal inverseBits[depth];
    signal leftNodes[depth];
    signal leftTermsA[depth];
    signal leftTermsB[depth];
    signal rightNodes[depth];
    signal rightTermsA[depth];
    signal rightTermsB[depth];
    component hashes[depth];

    levelHashes[0] <== leaf;

    for (var i = 0; i < depth; i++) {
        inverseBits[i] <== 1 - bits.out[i];
        leftTermsA[i] <== levelHashes[i] * inverseBits[i];
        leftTermsB[i] <== siblings[i] * bits.out[i];
        leftNodes[i] <== leftTermsA[i] + leftTermsB[i];
        rightTermsA[i] <== levelHashes[i] * bits.out[i];
        rightTermsB[i] <== siblings[i] * inverseBits[i];
        rightNodes[i] <== rightTermsA[i] + rightTermsB[i];

        hashes[i] = Poseidon(2);
        hashes[i].inputs[0] <== leftNodes[i];
        hashes[i].inputs[1] <== rightNodes[i];
        levelHashes[i + 1] <== hashes[i].out;
    }

    root <== levelHashes[depth];
}

template DealerPeekNoBlackjack(depth) {
    signal input handIdHash;
    signal input deckRoot;
    signal input holeCardIndex;
    signal input upcardClass;
    signal input peekResult;
    signal input chainHandId;
    signal input tableId;
    signal input wager;
    signal input dealerUpcard;
    signal input playerFirstCard;
    signal input playerSecondCard;

    signal input cardId;
    signal input cardSalt;
    signal input merkleSiblings[depth];

    component leaf = CardLeaf();
    leaf.cardId <== cardId;
    leaf.salt <== cardSalt;

    component merkle = PoseidonMerkleRoot(depth);
    merkle.leaf <== leaf.out;
    merkle.index <== holeCardIndex;
    for (var i = 0; i < depth; i++) {
        merkle.siblings[i] <== merkleSiblings[i];
    }
    merkle.root === deckRoot;

    component rank = CardRankFromId();
    rank.cardId <== cardId;

    component upcardIsAce = IsEqual();
    upcardIsAce.in[0] <== upcardClass;
    upcardIsAce.in[1] <== 1;

    component upcardIsTenValue = IsEqual();
    upcardIsTenValue.in[0] <== upcardClass;
    upcardIsTenValue.in[1] <== 2;
    (upcardIsAce.out + upcardIsTenValue.out) === 1;

    component dealerUpcardMatchesClass = IsEqual();
    dealerUpcardMatchesClass.in[0] <== dealerUpcard;
    dealerUpcardMatchesClass.in[1] <== 1;
    dealerUpcardMatchesClass.out === upcardIsAce.out;

    component holeIsAce = IsEqual();
    holeIsAce.in[0] <== rank.rank;
    holeIsAce.in[1] <== 1;

    component holeIs10 = IsEqual();
    component holeIs11 = IsEqual();
    component holeIs12 = IsEqual();
    component holeIs13 = IsEqual();
    holeIs10.in[0] <== rank.rank;
    holeIs10.in[1] <== 10;
    holeIs11.in[0] <== rank.rank;
    holeIs11.in[1] <== 11;
    holeIs12.in[0] <== rank.rank;
    holeIs12.in[1] <== 12;
    holeIs13.in[0] <== rank.rank;
    holeIs13.in[1] <== 13;

    signal holeIsTenValue;
    holeIsTenValue <== holeIs10.out + holeIs11.out + holeIs12.out + holeIs13.out;

    signal holeIsNonSpecial;
    holeIsNonSpecial <== 1 - holeIsAce.out - holeIsTenValue;

    signal upcardValue;
    upcardValue <== 11 * upcardIsAce.out + 10 * upcardIsTenValue.out;

    signal holeValue;
    holeValue <== 11 * holeIsAce.out + 10 * holeIsTenValue + rank.rank * holeIsNonSpecial;

    signal blackjackTotal;
    blackjackTotal <== holeValue + upcardValue;
    component totalIsBlackjack = IsEqual();
    totalIsBlackjack.in[0] <== blackjackTotal;
    totalIsBlackjack.in[1] <== 21;
    peekResult === totalIsBlackjack.out;

    signal computedHandHash0;
    signal computedHandHash1;
    signal computedHandHash2;
    signal computedHandHash3;
    signal computedHandHash4;
    signal computedHandHash5;
    component handHash0 = Poseidon(2);
    handHash0.inputs[0] <== chainHandId;
    handHash0.inputs[1] <== tableId;
    computedHandHash0 <== handHash0.out;
    component handHash1 = Poseidon(2);
    handHash1.inputs[0] <== computedHandHash0;
    handHash1.inputs[1] <== wager;
    computedHandHash1 <== handHash1.out;
    component handHash2 = Poseidon(2);
    handHash2.inputs[0] <== computedHandHash1;
    handHash2.inputs[1] <== deckRoot;
    computedHandHash2 <== handHash2.out;
    component handHash3 = Poseidon(2);
    handHash3.inputs[0] <== computedHandHash2;
    handHash3.inputs[1] <== dealerUpcard;
    computedHandHash3 <== handHash3.out;
    component handHash4 = Poseidon(2);
    handHash4.inputs[0] <== computedHandHash3;
    handHash4.inputs[1] <== playerFirstCard;
    computedHandHash4 <== handHash4.out;
    component handHash5 = Poseidon(2);
    handHash5.inputs[0] <== computedHandHash4;
    handHash5.inputs[1] <== playerSecondCard;
    computedHandHash5 <== handHash5.out;
    handIdHash === computedHandHash5;
}

component main { public [handIdHash, deckRoot, holeCardIndex, upcardClass, peekResult, chainHandId, tableId, wager, dealerUpcard, playerFirstCard, playerSecondCard] } = DealerPeekNoBlackjack(9);
