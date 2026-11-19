import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib import font_manager as fm
from matplotlib.patches import FancyBboxPatch, Rectangle
import numpy as np
F="/home/claude/microparquet/assets/fonts/"
for f in ["SourceSans3-Regular.ttf","SourceSans3-Semibold.ttf","SourceSans3-Bold.ttf"]: fm.fontManager.addfont(F+f)
plt.rcParams.update({"font.family":"Source Sans 3","font.size":11})
SURF="#fcfcfb"; INK="#0b0b0b"; SEC="#52514e"; MUT="#898781"; GRID="#e1e0d9"; BASE="#c3c2b7"
CAT=["#2a78d6","#eb6834","#1baf7a","#eda100","#e87ba4","#008300","#6250d6","#e34948"]
SEQ=["#cde2fb","#b7d3f6","#9ec5f4","#86b6ef","#6da7ec","#5598e7","#3987e5","#2a78d6","#256abf","#1c5cab","#184f95","#104281","#0d366b"]
NEG="#e34948"; POS="#2a78d6"; NEUTRAL="#b9b8b0"; CRIT="#d03b3b"; GOOD="#0ca30c"
def frame(ax, xgrid=False):
    ax.set_facecolor(SURF)
    for s in ["top","right","left"]: ax.spines[s].set_visible(False)
    ax.spines["bottom"].set_color(BASE); ax.spines["bottom"].set_linewidth(1)
    ax.tick_params(colors=MUT, length=0, labelsize=10)
    ax.grid(axis="x" if xgrid else "y", color=GRID, linewidth=1); ax.set_axisbelow(True)
def head(fig, title, sub, foot=None):
    fig.text(0.03,0.95,title,fontsize=15,fontweight="semibold",color=INK,va="top")
    fig.text(0.03,0.885,sub,fontsize=11,color=SEC,va="top")
    if foot: fig.text(0.03,0.025,foot,fontsize=9,color=MUT)
def rbar(ax,x,h,w,color,bottom=0,horizontal=False,r=0.035):
    # rounded data-end, square at the baseline
    if not horizontal:
        y0=bottom; top=bottom+h
        lo,hi=min(y0,top),max(y0,top)
        ax.add_patch(FancyBboxPatch((x-w/2,lo),w,hi-lo,boxstyle=f"round,pad=0,rounding_size={min(r*(ax.get_ylim()[1]-ax.get_ylim()[0]), (hi-lo)/2, w/2)}",ec="none",fc=color,mutation_aspect=1))
        # square the baseline end
        sq=(hi-lo)*0.5
        ax.add_patch(Rectangle((x-w/2, bottom if h>=0 else bottom-sq), w, sq, ec="none", fc=color))
    else:
        lo,hi=min(bottom,bottom+h),max(bottom,bottom+h)
        ax.add_patch(Rectangle((lo,x-w/2),hi-lo,w,ec="none",fc=color))
def save(fig,name): fig.savefig(f"/home/claude/p7/{name}.png",dpi=160,facecolor=SURF); plt.close(fig)

# 1 Gap ladder: two panels sharing x
b=["Sight","≤1M","1–3M","3–6M","6–12M","1–5Y",">5Y"]
net=[-1850,-620,410,780,920,2600,1400]; cum=np.cumsum(net); ta=26000; cpct=cum/ta*100; lim=[-5,-10,-10,None,None,None,None]
fig=plt.figure(figsize=(9,6.2),facecolor=SURF); head(fig,"Cumulative gap turns positive after 3–6 months","Liquidity gap ladder, BDT Crore · cutoff 31 Oct 2026","Limits from the ALCO policy file · synthetic data")
a1=fig.add_axes([0.08,0.47,0.88,0.32]); a2=fig.add_axes([0.08,0.10,0.88,0.28])
frame(a1); a1.set_xlim(-0.6,len(b)-0.4); a1.set_ylim(-2300,3000)
for i,v in enumerate(net): rbar(a1,i,v,0.42,POS if v>=0 else NEG)
for i,v in enumerate(net): a1.text(i, v+(120 if v>=0 else -120), f"{v:,.0f}", ha="center", va="bottom" if v>=0 else "top", fontsize=9.5, color=SEC)
a1.axhline(0,color=BASE,lw=1); a1.set_xticks(range(len(b))); a1.set_xticklabels([]); a1.set_ylabel("Net gap",color=SEC)
frame(a2); a2.set_xlim(-0.6,len(b)-0.4); a2.set_ylim(-15,22)
a2.plot(range(len(b)),cpct,color=CAT[0],lw=2,solid_capstyle="round",zorder=3)
a2.scatter(range(len(b)),cpct,s=36,color=CAT[0],edgecolor=SURF,linewidth=2,zorder=4)
for i,l in enumerate(lim):
    if l is not None: a2.plot([i-0.32,i+0.32],[l,l],color=CRIT,lw=1.5,solid_capstyle="butt")
a2.text(2.4,-10,"— red ticks: minimum cumulative gap (ALCO limit)",color=MUT,fontsize=9,va="center")
a2.scatter([0],[cpct[0]],s=120,facecolor="none",edgecolor=CRIT,linewidth=1.5,zorder=5)
a2.text(-0.25,cpct[0]+3.2,f"Breach: {cpct[0]:.1f}% vs −5% limit",color=INK,fontsize=9.5,va="bottom")
a2.annotate(f"{cpct[-1]:.1f}%",(len(b)-1,cpct[-1]),xytext=(6,0),textcoords="offset points",va="center",fontsize=9.5,color=INK)
a2.axhline(0,color=BASE,lw=1); a2.set_xticks(range(len(b))); a2.set_xticklabels(b,color=SEC); a2.set_ylabel("Cumulative gap, % of assets",color=SEC)
save(fig,"01_gap_ladder")

# 2 NII bridge waterfall
steps=[("Sep interest",80.0,"total"),("Volume",8.0,"d"),("Rate",5.0,"d"),("Mix",0.5,"d"),("Oct interest",93.5,"total")]
fig=plt.figure(figsize=(9,5.2),facecolor=SURF); head(fig,"Higher balances explain most of the +13.5 Cr rise in TD interest","Annualised interest, BDT Crore · Sep → Oct 2026","Volume = ΔBalance × prior rate · Rate = prior balance × Δrate · Mix = ΔBalance × Δrate")
ax=fig.add_axes([0.08,0.12,0.88,0.66]); frame(ax); ax.set_xlim(-0.6,4.6); ax.set_ylim(0,100)
run=0
for i,(n,v,k) in enumerate(steps):
    if k=="total":
        rbar(ax,i,v,0.44,"#52514e" if i==0 else "#0d366b"); run=v; lab=f"{v:.2f}"; y=v
    else:
        rbar(ax,i,v,0.44,POS if v>=0 else NEG,bottom=run); 
        ax.plot([i-1+0.22,i-0.22],[run,run],color=BASE,lw=1)
        run+=v; lab=f"+{v:.2f}"; y=run
    ax.text(i,y+1.5,lab,ha="center",va="bottom",fontsize=10,color=INK if k=="total" else SEC)
ax.set_xticks(range(5)); ax.set_xticklabels([s[0] for s in steps],color=SEC)
save(fig,"02_nii_bridge")

# 3 CL migration heatmap (% of previous grade's balance)
g=["STD","SMA","SS","DF","B/L"]; cols=g+["Closed"]
M=np.array([[93.1,3.2,0.9,0.2,0.1,2.5],[18.0,61.5,14.3,2.1,0.4,3.7],[4.2,3.0,70.8,16.9,2.6,2.5],[1.1,0.5,2.0,79.4,15.8,1.2],[0.3,0.0,0.2,0.6,97.9,1.0]])
fig=plt.figure(figsize=(9,5.6),facecolor=SURF); head(fig,"Most SS loans stayed put; 17% slipped to DF","CL migration, % of each September grade's balance → October grade","Diagonal = no change · synthetic data")
ax=fig.add_axes([0.13,0.08,0.84,0.70]); ax.set_facecolor(SURF)
for s in ax.spines.values(): s.set_visible(False)
ax.tick_params(length=0,colors=SEC)
for r in range(5):
    for c in range(6):
        v=M[r,c]; idx=min(12,int(v/100*12.99)); col=SEQ[idx]
        ax.add_patch(FancyBboxPatch((c+0.04,4-r+0.04),0.92,0.92,boxstyle="round,pad=0,rounding_size=0.06",fc=col,ec="none",lw=0))
        ax.text(c+0.5,4-r+0.5,f"{v:.1f}",ha="center",va="center",fontsize=10,color=("#ffffff" if idx>=6 else INK))
ax.set_xlim(0,6); ax.set_ylim(0,5); ax.set_xticks(np.arange(6)+0.5); ax.set_xticklabels(cols); ax.xaxis.tick_top()
ax.set_yticks(np.arange(5)+0.5); ax.set_yticklabels(g[::-1]); ax.set_ylabel("September grade",color=SEC)
save(fig,"03_cl_migration")

# 4 IRRBB six scenarios
sc=["Parallel up","Short up","Steepener","Flattener","Short down","Parallel down"]; v=[-10.50,-4.48,-2.17,-0.23,4.82,12.57]
fig=plt.figure(figsize=(9,5.2),facecolor=SURF); head(fig,"Parallel up is the worst case at −10.5% of Tier 1","ΔEVE by BB scenario, % of Tier 1 capital · outlier test at −15%","BRPD IRRBB guideline 2021, Table 2 shocks · synthetic book")
ax=fig.add_axes([0.17,0.10,0.78,0.68]); frame(ax,xgrid=True); ax.set_xlim(-17,15); ax.set_ylim(-0.6,5.6)
for i,(n,x) in enumerate(zip(sc,v)):
    y=5-i; rbar(ax,y,x,0.42,NEG if x<0 else POS,horizontal=True)
    ax.text(x+(-0.4 if x<0 else 0.4),y,f"{x:+.2f}%",ha="right" if x<0 else "left",va="center",fontsize=10,color=SEC)
ax.axvline(0,color=BASE,lw=1); ax.axvline(-15,color=CRIT,lw=1.5); ax.text(-14.8,5.45,"Outlier −15%",color=CRIT,fontsize=9.5)
ax.set_yticks([5-i for i in range(6)]); ax.set_yticklabels(sc,color=SEC); ax.spines["bottom"].set_visible(False)
save(fig,"04_irrbb_scenarios")

# 5 Yield curves by date
ten=np.array([1,3,6,12,24,36,60,120]); lab=["1M","3M","6M","1Y","2Y","3Y","5Y","10Y"]
curves={"Jan 2026":[8.1,8.4,8.8,9.2,9.6,9.8,10.1,10.4],"Apr 2026":[8.6,8.9,9.3,9.7,10.0,10.2,10.4,10.6],"Jul 2026":[9.4,9.6,9.9,10.2,10.4,10.5,10.6,10.8],"Oct 2026":[9.9,10.1,10.3,10.5,10.6,10.6,10.7,10.8]}
fig=plt.figure(figsize=(9,5.2),facecolor=SURF); head(fig,"The curve rose and flattened through 2026","Treasury yield curves, % · latest highlighted","Curves from portable_data/curves · synthetic")
ax=fig.add_axes([0.08,0.12,0.80,0.66]); frame(ax); x=np.arange(len(ten))
for k,(n,y) in enumerate(curves.items()):
    last=(n=="Oct 2026"); c=CAT[0] if last else "#c3c2b7"
    ax.plot(x,y,color=c,lw=2 if last else 1.5,solid_capstyle="round",zorder=3 if last else 2)
    if last: ax.text(x[-1]+0.15,y[-1],n,va="center",fontsize=9.5,color=INK)
    else: ax.text(x[0]-0.15,y[0],n,va="center",ha="right",fontsize=9.5,color=MUT)
ax.scatter(x,curves["Oct 2026"],s=36,color=CAT[0],edgecolor=SURF,linewidth=2,zorder=4)
ax.set_xticks(x); ax.set_xticklabels(lab,color=SEC); ax.set_xlim(-1.4,len(x)+0.2); ax.set_ylim(7.8,11.2)
save(fig,"05_yield_curves")

# 6 Deposit mix stacked columns
mon=["May","Jun","Jul","Aug","Sep","Oct"]; parts={"Term deposits":[620,640,655,668,690,712],"Savings":[260,262,258,266,270,272],"Current":[140,151,149,158,162,170],"Other":[45,46,44,48,47,49]}
fig=plt.figure(figsize=(9,5.4),facecolor=SURF); head(fig,"Term deposits drove growth; CASA held at 37–38% of deposits","Deposits by product, BDT Crore · month-end","Synthetic data")
ax=fig.add_axes([0.08,0.12,0.70,0.66]); frame(ax); ax.set_xlim(-0.6,5.6); ax.set_ylim(0,1300)
bot=np.zeros(6); gap=4
for k,(n,vals) in enumerate(parts.items()):
    for i,val in enumerate(vals):
        ax.add_patch(Rectangle((i-0.21,bot[i]+(gap if k else 0)),0.42,val-(gap if k else 0),fc=CAT[k],ec="none"))
    bot+=np.array(vals)
for i,t in enumerate(bot): ax.text(i,t+18,f"{t:,.0f}",ha="center",fontsize=9.5,color=SEC)
ax.set_xticks(range(6)); ax.set_xticklabels(mon,color=SEC)
for k,n in enumerate(parts):
    fig.patches.append(Rectangle((0.80,0.70-k*0.06),0.014,0.025,transform=fig.transFigure,fc=CAT[k],ec="none"))
    fig.text(0.822,0.712-k*0.06,n,fontsize=10,color=INK,va="center")
save(fig,"06_deposit_mix")
