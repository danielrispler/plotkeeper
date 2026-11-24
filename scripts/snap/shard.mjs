// `snap:all --shard=<i>/<n>` (#432): the fixtures one of n CI jobs renders, every n-th from the i-th, so the n shards together
// render each fixture once. Without the flag, every fixture. A malformed flag throws, naming the form it takes.
export const shard = (names, flag) => {
  if (flag === undefined) return names;
  const usage = new Error(`usage: --shard=<i>/<n>, 1 <= i <= n (got "${flag}")`);
  const match = /^(\d+)\/(\d+)$/.exec(flag);
  if (!match) throw usage;
  const [i, n] = [Number(match[1]), Number(match[2])];
  if (i < 1 || i > n) throw usage;
  return names.filter((_, k) => k % n === i - 1);
};
