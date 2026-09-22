// Standalone research accelerator. All signals come from the Python app code.
// Output is only holdings; accounting and validation remain in Python.
use std::{env, fs, path::Path, thread};

fn bytes(root: &Path, name: &str) -> Vec<u8> { fs::read(root.join(name)).unwrap() }

fn main() {
    let args: Vec<String> = env::args().collect();
    let root = Path::new(&args[1]);
    let dims: Vec<usize> = fs::read_to_string(root.join("dimensions.txt")).unwrap()
        .split_whitespace().map(|s|s.parse().unwrap()).collect();
    let [nk, np, nt, na, nc, nw, begin, width]: [usize;8] = dims.try_into().unwrap();
    let pairs = bytes(root,"pairs.bin");
    let active = bytes(root,"active.bin");
    let orders = bytes(root,"orders.bin");
    let gates = bytes(root,"gates.bin");
    let configs = bytes(root,"configs.bin");
    let groups = bytes(root,"groups.bin");
    let signals: Vec<Vec<u8>> = (0..nw).map(|i|bytes(root,&format!("votes-{i}.bin"))).collect();
    let present: Vec<Vec<u8>> = (0..nw).map(|i|bytes(root,&format!("present-{i}.bin"))).collect();
    assert_eq!(orders.len(),nt*width);
    assert_eq!(configs.len(),nc*nk);
    let workers = 4.min(nc);
    thread::scope(|scope| {
        for worker in 0..workers {
            let first=worker*nc/workers;
            let last=(worker+1)*nc/workers;
            let signals=&signals; let present=&present; let pairs=&pairs;
            let active=&active; let orders=&orders; let gates=&gates;
            let configs=&configs; let groups=&groups;
            scope.spawn(move || {
                let mut out=vec![255u8;(last-first)*(nt-begin)];
                for trial in first..last {
                    let members:Vec<usize>=configs[trial*nk..(trial+1)*nk].iter()
                        .filter(|&&k|k!=255).map(|&k|k as usize).collect();
                    let group=groups[trial] as usize;
                    let votes=&signals[group];
                    let exists=&present[group];
                    let mut scores=vec![0u8;nt*na];
                    for pair in 0..np {
                        let a=pairs[2*pair] as usize;
                        let b=pairs[2*pair+1] as usize;
                        let mut state=false;
                        let mut sum=vec![0i16;nt];
                        for &member in &members {
                            let offset=(member*np+pair)*nt;
                            for t in 0..nt { sum[t]+=votes[offset+t] as i8 as i16; }
                        }
                        for t in 0..nt {
                            if sum[t]>0 { state=true; } else if sum[t]<0 { state=false; }
                            if active[pair*nt+t]!=0 && gates[group*nt+t]!=0 {
                                let bull=state && exists[pair*nt+t]!=0;
                                scores[t*na+if bull {a} else {b}]+=1;
                            }
                        }
                    }
                    let cash=(na-1) as u8;
                    let mut pending=255u8;
                    let mut current=255u8;
                    for t in begin..nt {
                        let order=&orders[t*width..(t+1)*width];
                        if pending!=255 && pending!=cash && !order.contains(&pending) { pending=cash; }
                        if pending!=255 { current=pending; }
                        out[(trial-first)*(nt-begin)+(t-begin)]=current;
                        if gates[group*nt+t]==0 { pending=cash; continue; }
                        let mut best=order[0];
                        for &asset in &order[1..] {
                            if asset!=255 && scores[t*na+asset as usize]>scores[t*na+best as usize] { best=asset; }
                        }
                        pending=best;
                    }
                    if (trial-first+1)%2000==0 { eprintln!("worker {worker}: {} / {}",trial-first+1,last-first); }
                }
                fs::write(root.join(format!("held-{worker}.bin")),out).unwrap();
            });
        }
    });
    eprintln!("DONE: {nc} portfolios, {} bars",nt-begin);
}
