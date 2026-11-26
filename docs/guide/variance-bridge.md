# Variance bridge (volume, rate and mix)

## 1. What it does

The third tab of the MoM variance tool, "MoM bridge". It splits the change in interest (or in NII, or in cost of funds) between two months into three effects that **add up exactly** to the change: **volume** (the total balance moved, at last month's average rate), **mix** (balance moved between products of different rates) and **rate** (each product's own rate moved, on this month's balance). Without a rate column it shows the change of balance by key.

## 2. When to use it

- The ALCO question "interest income rose by X: how much is more balance, how much is a better rate, how much is a better mix?".
- A waterfall (bridge) chart for the ALCO pack or the PowerPoint.

## 3. How to use it

1. Open last month's and this month's tables, with the same column names, balances in BDT and the rate in percent (9.5 = 9.5%).
2. Banking ▸ Treasury & ALM ▸ "MoM variance and commentary…" ([MoM variance](mom-variance.md)): choose both tabs, the Key, the Balance and the Rate %, and Build.
3. Three tabs open; open **MoM bridge**.
4. To draw it, use the chart studio's waterfall or the NII bridge template ([NII bridge chart](chart-nii-bridge.md)): every export works.
5. The tab states the three formulas it uses. The earlier MoM variance table keeps its own (joint) mix column, so its numbers are unchanged; the two splits differ by design.

## 4. Example

Last month: product A 100 at 10% (interest 10), B 100 at 6% (6). This month: A 120 at 11% (13.2), B 80 at 6% (4.8). Total balance 200 both months, average rate before 8%, interest 16 then 18, change +2.0.

| Effect | Working | Amount |
| --- | --- | ---: |
| Volume | (200 − 200) × 8% | 0.0 |
| Mix | (120 × 10% + 80 × 6%) − 200 × 8% = 16.8 − 16 | 0.8 |
| Rate | 120 × (11% − 10%) + 80 × (6% − 6%) | 1.2 |
| Total | | 2.0 |

## 5. What it refuses and why

- A negative balance: refused. Use Reverse sign in the schema first ([Keep or drop columns, and reverse a sign](schema-keep-reverse-sign.md)).
- Interest with no balance, or no balance at all in the earlier month: refused.
- If the three effects do not add up to the change, the run stops: a gap is an error, never hidden.
- A new product (no balance before) is taken to have had the average rate before, so its rate gap is in the rate effect; a closed product has no rate effect.

## 6. Python export

Not translated: the bridge is a result tab. Export the tab itself.

## 7. Saved in

As a result tab: in the project and in Saved tool runs ([Saved tool runs](saved-tool-runs.md)).

## 8. Related tools

[MoM variance](mom-variance.md) · [NII bridge chart](chart-nii-bridge.md) · [Rate shock and ΔNII](rate-shock-nii.md) · [Report packs](report-packs.md)
