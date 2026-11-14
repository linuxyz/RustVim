vim9script
# p14: the fourteen valid func-typed lambda forms from vim9.txt vim9-lambda.
const I01 = (n) => n + 1
const I02 = (n): number => n + 1
const I03 = (n: number) => n + 1
const I04 = (n: number): number => n + 1
const I05: func = (n) => n + 1
const I06: func = (n: number) => n + 1
const I07: func = (n): number => n + 1
const I08: func = (n: number): number => n + 1
const I09: func: number = (n: number) => n + 1
const I10: func: number = (n): number => n + 1
const I11: func: number = (n: number): number => n + 1
const I12: func(number): number = (n): number => n + 1
const I13: func(number): number = (n: number) => n + 1
const I14: func(number): number = (n: number): number => n + 1
echomsg 'fourteen=' .. 0->I01()->I02()->I03()->I04()->I05()->I06()->I07()->I08()->I09()->I10()->I11()->I12()->I13()->I14()
const Add = (a: number, b: number): number => a + b
const Block = (n: number): list<number> => {
  var result: list<number> = []
  for i in range(n)
    result->add(i)
  endfor
  return result
}
echomsg 'add=' .. Add(2, 3)
echomsg 'filtered=' .. string([8, -2, 9]->filter((_, val) => val > 0))
echomsg 'block=' .. string(Block(3))
