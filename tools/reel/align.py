
"""Per-line take selection over ASR chunks. Tolerates wording drift / Hinglish: fuzzy word similarity + DP."""
import json,re,difflib
NUM={'200':'two hundred','0':'zero'}
def norm_words(t):
    t=t.lower().replace('’',"'").replace('—',' ').replace('…',' ')
    for k,v in NUM.items(): t=re.sub(rf'\b{k}\b',v,t)
    t=t.replace("you're","you are").replace("it's","it is").replace("i've","i have")
    return [w for w in re.findall(r"[a-z0-9']+",t) if w]
def wsim(a,b):
    if a==b: return 1.0
    return difflib.SequenceMatcher(None,a,b).ratio()
def line_score(script,heard):
    S=norm_words(script); H=norm_words(heard)
    if not S or not H: return 0.0,0.0
    # soft LCS-ish alignment: DP over words with fuzzy match weight
    n,m=len(S),len(H); D=[[0.0]*(m+1) for _ in range(n+1)]
    for i in range(1,n+1):
        for j in range(1,m+1):
            s=wsim(S[i-1],H[j-1]); s=s if s>=0.6 else 0
            D[i][j]=max(D[i-1][j],D[i][j-1],D[i-1][j-1]+s)
    match=D[n][m]; cov=match/n; prec=match/m
    f=2*cov*prec/(cov+prec+1e-9)
    return f,cov
def select(chunks,script,max_span=3,min_score=0.45):
    """chunks: [{start,end,text,lp}] sorted. Returns per line: dict(line,chunks=[i..j],start,end,score,cov,alts)."""
    N=len(chunks); L=len(script)
    cand={}  # (line, i, j) -> score
    for li,s in enumerate(script):
        for i in range(N):
            for j in range(i,min(N,i+max_span)):
                if chunks[j]['start']-chunks[j-1 if j>i else i]['end']>2.5 and j>i: break
                text=' '.join(c['text'] for c in chunks[i:j+1])
                f,cov=line_score(s,text)
                lp=sum(c['lp'] for c in chunks[i:j+1])/(j-i+1)
                sc=f*(0.75+0.25*cov)+0.15*max(-1,min(0,lp+0.3))-0.012*(j-i)+0.004*(i/N)   # clarity & later-take tie-break
                if f>=min_score: cand[(li,i,j)]=(sc,f,cov,lp)
    # DP: lines in order, chunk spans strictly increasing; a line may be missing (penalty)
    import functools,sys
    sys.setrecursionlimit(10000)
    @functools.lru_cache(None)
    def best(li,start):
        if li==L: return 0.0,()
        opts=[(best(li+1,start)[0]-0.6,best(li+1,start)[1],None)]  # skip line
        for (l,i,j),v in cand.items():
            if l==li and i>=start:
                r=best(li+1,j+1); opts.append((v[0]+r[0],r[1],(i,j)))
        sc,rest,pick=max(opts,key=lambda o:o[0])
        return sc,((pick,)+rest)
    _,picks=best(0,0)
    out=[]
    for li,p in enumerate(picks):
        alts=sorted([(v[0],i,j) for (l,i,j),v in cand.items() if l==li],reverse=True)[:4]
        if p is None: out.append(dict(line=li,text=script[li],missing=True,alts=alts)); continue
        i,j=p; v=cand[(li,i,j)]
        out.append(dict(line=li,text=script[li],chunks=list(range(i,j+1)),start=chunks[i]['start'],end=chunks[j]['end'],
            heard=' '.join(c['text'] for c in chunks[i:j+1]),score=round(v[0],3),f=round(v[1],3),cov=round(v[2],3),lp=round(v[3],3),
            alts=[(round(a[0],3),chunks[a[1]]['start'],chunks[a[2]]['end']) for a in alts]))
    return out
