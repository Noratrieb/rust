#data = [15, 176602, 26476, 29319, 7809, 2786, 1158, 347, 180, 42, 29, 17, 15, 13, 8, 3, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
#total = sum(data)
#
#for i in range(16):
#    until = sum(data[:i])
#    print(f"{2**(i-1): 7}..={2**i: 7}: {until: 7} ({(until/total)*100:.5}%)")

file = open("random").read()

file = "\n".join(filter(lambda line: line.startswith("BACKTRACE"), file.split("\n")))

backtraces = file.split("BACKTRACE START: ")
print(len(backtraces))

counts = dict()

for bt in backtraces:
    if bt in counts:
        counts[bt] += 1
    else:
        counts[bt] = 1

elems = list(counts.items())
elems.sort(key=lambda elem: elem[1], reverse=True)

for elem in elems[0:10]:
    print(f"BACKTRACE COUNT {elem[1]}: {elem[0]}")
