// Removed from ui/times.ts in #415: `timeNode`, which nothing imported.

// Time `time`'s node, which counts up.
export const timeNode = ([title, draw]: Time) => Object.assign(counts(document.createElement("span"), draw), { title });
