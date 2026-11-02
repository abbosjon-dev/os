import socket, json, subprocess, time, os, sys
KERNEL=sys.argv[1]; OUT=sys.argv[2]
QMP="/tmp/claude-0/qmp.sock"; PPM="/tmp/claude-0/screen.ppm"
for f in (QMP,PPM,OUT):
    try: os.remove(f)
    except: pass
disk="/tmp/claude-0/gfx-disk.img"
if not os.path.exists(disk): subprocess.run(["truncate","-s","64M",disk])
q=subprocess.Popen(["qemu-system-aarch64","-machine","virt","-cpu","max","-smp","4","-m","512M",
  "-device","virtio-gpu-device","-display","none",
  "-drive",f"file={disk},if=none,format=raw,id=s","-device","virtio-blk-device,drive=s",
  "-device","virtio-rng-device",
  "-qmp",f"unix:{QMP},server,nowait","-serial","file:/tmp/claude-0/gfx-serial.txt",
  "-kernel",KERNEL], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1)
s=socket.socket(socket.AF_UNIX); 
for _ in range(50):
    try: s.connect(QMP); break
    except: time.sleep(0.2)
f=s.makefile('rw')
f.readline() # greeting
f.write(json.dumps({"execute":"qmp_capabilities"})+"\n"); f.flush(); f.readline()
wait=float(sys.argv[3]) if len(sys.argv)>3 else 9
time.sleep(wait)
f.write(json.dumps({"execute":"screendump","arguments":{"filename":PPM}})+"\n"); f.flush()
print("screendump:",f.readline().strip()[:80])
time.sleep(0.5)
f.write(json.dumps({"execute":"quit"})+"\n"); f.flush()
q.wait(timeout=10)
subprocess.run(["convert",PPM,OUT],check=True)
print("saved",OUT, os.path.getsize(OUT),"bayt")
