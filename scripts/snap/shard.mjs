// `snap:all --shard=<i>/<n>` (#432): the fixtures one of n CI jobs renders, every n-th from the i-th, so the n shards together
// render each fixture once. Without the flag, every fixture. A malformed flag throws, naming the form it takes.
export const shard = (names, flag) => {
  if (flag === undefined) return names;
  const match = /^(\d+)\/(\d+)$/.exec(flag);
  const [i, n] = match ? [Number(match[1]), Number(match[2])] : [0, 0];
  if (n < 1 || i < 1 || i > n) throw new Error(`usage: --shard=<i>/<n>, 1 <= i <= n (got "${flag}")`);
  return names.filter((_, k) => k % n === i - 1);
};
