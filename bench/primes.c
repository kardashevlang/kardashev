#include <stdio.h>
int main(){ long n=200000,count=0;
  for(long i=2;i<n;i++){ int p=1; for(long d=2;d*d<=i;){ if(i%d==0){p=0;d=i+1;} else {d=d+1;} } if(p)count++; }
  printf("%ld\n",count); return 0; }
