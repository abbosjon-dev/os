import os,sys,pty,time,select,socket,json,subprocess
# Foydalanish: shot.py KERNEL OUT.png [fresh] [--size WxH] [--vps]
#   --size WxH — virtio-gpu ekran ölçami (xres/yres; yadro GET_DISPLAY_INFO orqali oladi)
#   --vps      — Netcup VPS yöli: UEFI (BOOTAA64.EFI) + ACPI + GICv3 + virtio-*-pci
#   CLICKS dagi "@nom" (browser, max, addr, home, center, park) — zgeom.py dan, ekran ölçamiga qarab;
#   "u,URL" — manzil satriga URL yozib öçiş.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zgeom
argv=[]; SW,SH=1280,800; VPS=False
_a=iter(sys.argv[1:])
for x in _a:
    if x=="--size": SW,SH=map(int,next(_a).lower().split("x"))
    elif x.startswith("--size="): SW,SH=map(int,x[7:].lower().split("x"))
    elif x=="--vps": VPS=True
    else: argv.append(x)
KERNEL=os.path.abspath(argv[0]); OUT=argv[1]; FRESH=len(argv)>2 and argv[2]=="fresh"
GEO=zgeom.metrics(SW,SH)
QMP="/tmp/claude-0/qmpg.sock"; PPM="/tmp/claude-0/sg.ppm"; disk="/tmp/claude-0/shot-disk.img"
for p in (QMP,PPM,OUT):
    try: os.remove(p)
    except: pass
if FRESH:
    try: os.remove(disk)
    except: pass
if not os.path.exists(disk): subprocess.run(["truncate","-s","128M",disk])
RES=f"xres={SW},yres={SH}"
if VPS:
    fw=next((f for f in ("/usr/share/qemu-efi-aarch64/QEMU_EFI.fd","/usr/share/AAVMF/AAVMF_CODE.fd") if os.path.exists(f)),None)
    if not fw: sys.exit("shot.py --vps: UEFI firmware yöq")
    esp="/tmp/claude-0/shot-esp.img"
    here=os.path.dirname(os.path.abspath(__file__))
    subprocess.run(["sh",os.path.join(here,"..","efi","mkesp.sh"),KERNEL,esp],check=True,stdout=subprocess.DEVNULL)
    cmd=["qemu-system-aarch64","-machine","virt,gic-version=3","-cpu","max","-smp","4","-m","1G","-bios",fw,
      "-drive",f"file={esp},if=none,format=raw,id=esp","-device","virtio-blk-pci,drive=esp,bootindex=0",
      "-drive",f"file={disk},if=none,format=raw,id=s","-device","virtio-blk-pci,drive=s",
      "-device",f"virtio-gpu-pci,{RES}","-device","virtio-keyboard-pci","-device","virtio-tablet-pci","-display","none",
      "-device","virtio-rng-pci","-nic","user,model=virtio-net-pci","-qmp",f"unix:{QMP},server,nowait",
      "-serial","mon:stdio"]
else:
    cmd=["qemu-system-aarch64","-machine","virt","-cpu","max","-smp","4","-m","512M",
      "-device",f"virtio-gpu-device,{RES}","-device","virtio-keyboard-device","-device","virtio-tablet-device","-display","none",
      "-drive",f"file={disk},if=none,format=raw,id=s","-device","virtio-blk-device,drive=s",
      "-device","virtio-rng-device","-netdev","user,id=n","-device","virtio-net-device,netdev=n","-qmp",f"unix:{QMP},server,nowait",
      "-serial","mon:stdio","-kernel",KERNEL]
pid,fd=pty.fork()
if pid==0:
    os.execvp(cmd[0],cmd); os._exit(1)
buf=b""
def pump(tmo=0.2):
    global buf
    r,_,_=select.select([fd],[],[],tmo)
    if r:
        try: buf+=os.read(fd,65536)
        except OSError: pass
def waitfor(subs,t=50):
    s=time.time()
    while time.time()-s<t:
        pump()
        if any(x in buf[-800:].decode('latin1') for x in subs): return True
    return False
def send(x): os.write(fd,x.encode()); time.sleep(0.2)
if FRESH:
    waitfor(["Choose your language"],150 if VPS else 70)
    send("3\n"); waitfor(["full name"],25)
    send("Test User\n"); waitfor(["Username","sername","Login:"],25)
    send("test\n"); waitfor(["Password:"],25)
    send("pass1234\n"); waitfor(["Repeat","epeat"],25)
    send("pass1234\n")
else:
    if waitfor(["Password","assword"],150 if VPS else 70): send("pass1234\n")
# give GUI time to render
for _ in range(60): pump(0.25)
time.sleep(6)
qs=socket.socket(socket.AF_UNIX)
for _ in range(100):
    try: qs.connect(QMP); break
    except: time.sleep(0.2)
qf=qs.makefile('rw'); qf.readline()
qf.write(json.dumps({"execute":"qmp_capabilities"})+"\n"); qf.flush(); qf.readline()
def qcmd(c):
    qf.write(json.dumps(c)+"\n"); qf.flush(); return qf.readline()
def move(x,y):
    qcmd({"execute":"input-send-event","arguments":{"events":[
      {"type":"abs","data":{"axis":"x","value":int(x*32768/SW)}},
      {"type":"abs","data":{"axis":"y","value":int(y*32768/SH)}}]}})
def click(x,y):
    move(x,y); time.sleep(0.3)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":True,"button":"left"}}]}})
    time.sleep(0.1)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":False,"button":"left"}}]}})
    time.sleep(1.5)
def raw_click(x,y):
    move(x,y); time.sleep(0.15)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":True,"button":"left"}}]}})
    time.sleep(0.05)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":False,"button":"left"}}]}})
SNAPS=[]
def snap(name):
    pp="/tmp/claude-0/snap_"+name+".ppm"
    qcmd({"execute":"screendump","arguments":{"filename":pp}})
    SNAPS.append((pp,name))
def drag(x1,y1,x2,y2):
    move(x1,y1); time.sleep(0.3)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":True,"button":"left"}}]}})
    time.sleep(0.3)
    for k in range(1,13):
        move(x1+(x2-x1)*k//12, y1+(y2-y1)*k//12); time.sleep(0.15)
    qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":False,"button":"left"}}]}})
    time.sleep(1.2)
QC={':':('semicolon',True),' ':'spc','\n':'ret','-':'minus','.':'dot','/':'slash','~':('grave_accent',True),'*':('8',True),'(':('9',True),')':('0',True),'+':('equal',True)}
def key(code, shift=False):
    ev=[]
    if shift: ev.append({"type":"key","data":{"down":True,"key":{"type":"qcode","data":"shift"}}})
    ev.append({"type":"key","data":{"down":True,"key":{"type":"qcode","data":code}}})
    ev.append({"type":"key","data":{"down":False,"key":{"type":"qcode","data":code}}})
    if shift: ev.append({"type":"key","data":{"down":False,"key":{"type":"qcode","data":"shift"}}})
    qcmd({"execute":"input-send-event","arguments":{"events":ev}}); time.sleep(0.08)
def typetext(txt):
    for ch in txt:
        v=QC.get(ch)
        if v is None:
            if ch.isalnum(): key(ch.lower(), ch.isupper())
        elif isinstance(v,tuple): key(v[0],v[1])
        else: key(v)
    time.sleep(0.8)
def geo_sub(c):
    # "@nom" → "x,y" (zgeom.py, joriy ekran ölçami)
    return ",".join(("%d,%d" % GEO[p[1:]]) if p.startswith("@") else p for p in c.split(","))
for c in os.environ.get("CLICKS","").split(";"):
    if c.strip():
        a=geo_sub(c.strip()).split(",")
        if a[0]=="u":
            click(*GEO["addr"])
            for _ in range(40): key("backspace")
            typetext(",".join(a[1:])); key("ret"); time.sleep(2.5)
        elif a[0]=="m": move(int(a[1]),int(a[2])); time.sleep(0.8)
        elif a[0]=="d": drag(int(a[1]),int(a[2]),int(a[3]),int(a[4]))
        elif a[0]=="2": raw_click(int(a[1]),int(a[2])); time.sleep(0.12); raw_click(int(a[1]),int(a[2])); time.sleep(1.5)
        elif a[0]=="c0": raw_click(int(a[1]),int(a[2]))
        elif a[0]=="snap": snap(a[1])
        elif a[0]=="t": typetext(a[1])
        elif a[0]=="r":
            move(int(a[1]),int(a[2])); time.sleep(0.3)
            qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":True,"button":"right"}}]}}); time.sleep(0.1)
            qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":False,"button":"right"}}]}}); time.sleep(1.2)
        elif a[0] in ("kd","ku","kp"):
            def kev(down): qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":down,"key":{"type":"qcode","data":a[1]}}}]}})
            if a[0] in ("kd","kp"): kev(True); time.sleep(0.08)
            if a[0] in ("ku","kp"): kev(False)
            time.sleep(0.6)
        elif a[0] in ("bd","bu"):
            qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":a[0]=="bd","button":"left"}}]}}); time.sleep(0.4)
        elif a[0]=="mv":
            x1,y1,x2,y2=map(int,a[1:5])
            for k in range(1,9): move(x1+(x2-x1)*k//8, y1+(y2-y1)*k//8); time.sleep(0.15)
            time.sleep(0.8)
        elif a[0]=="wd":
            for _ in range(int(a[1])):
                qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":True,"button":"wheel-down"}}]}})
                qcmd({"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":False,"button":"wheel-down"}}]}})
                time.sleep(0.1)
            time.sleep(0.8)
        elif a[0]=="w": time.sleep(float(a[1]))
        else: click(int(a[0]),int(a[1]))
time.sleep(float(os.environ.get("KEYS_DELAY","0")))
for ch in os.environ.get("KEYS","").replace("\\n","\n"):
    v=QC.get(ch)
    if v is None:
        if ch.isalnum(): key(ch.lower(), ch.isupper())
    elif isinstance(v,tuple): key(v[0],v[1])
    else: key(v)
    if ch=="\n": time.sleep(1.5)
time.sleep(2)
time.sleep(float(os.environ.get("WAIT_AFTER","0")))
qf.write(json.dumps({"execute":"screendump","arguments":{"filename":PPM}})+"\n"); qf.flush()
print("dump:",qf.readline().strip()[:60]); time.sleep(0.8)
subprocess.run(["convert",PPM,OUT],check=True)
for pp,nm in SNAPS: subprocess.run(["convert",pp,OUT.replace(".png","_"+nm+".png")]); print("saved",OUT,os.path.getsize(OUT))
for _ in range(10): pump(0.05)
open(OUT+".log","wb").write(buf)
qf.write(json.dumps({"execute":"quit"})+"\n"); qf.flush()
time.sleep(1); os.kill(pid,9)
